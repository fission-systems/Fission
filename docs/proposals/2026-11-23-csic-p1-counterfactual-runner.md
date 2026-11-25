# CSIC P0/P1/P2: contract, fork reality, a counterfactual runner and a solver-backed joint search

**RFC:** #1101, Track B (CSIC, counterfactual machine-state causality)
**Crate:** `crates/fission-causal` (experimental; no production crate depends on it)
**Status:** P0, P1 and P2 done on this crate's own fixtures (P2: section 8). Evidence is mechanical plus
native execution of the fixtures. Nothing here is adopted, and decompiler output
does not change.

## 1. What was built

- A v1 **contract** (`src/contract.rs`): the sink predicate Φ, legal interventions,
  a replay fingerprint, and six verdicts.
- A single-step **recorder** (`src/trace.rs`). It records each retired instruction's
  register and memory reads and writes (registers from p-code, memory from the
  emulator's own memory trace), syscalls, delivered input bytes and dynamic
  control parents.
- **Static control dependence** (`src/cfg.rs`): per-function CFGs, postdominators
  and Ferrante–Ottenstein–Warren control dependence, all from p-code.
- A **checkpoint fork** (`Replayer` in `src/runner.rs`): a `MachineState` clone plus
  the emulator fields a run changes, restored into a pooled emulator. Every
  checkpoint is validated by a null-intervention replay before it is used.
- The **one-variable runner** (`run_case`):
  1. record the incident;
  2. apply the determinism gates;
  3. slice backward;
  4. test candidates;
  5. verify every flip with a fresh replay from the initial state;
  6. run a bounded pair search when no single-location witness exists.
- **Baselines**:
  - Fission's own taint engine, as direct data and as data plus control;
  - the runner's own dynamic slice before any intervention;
  - a pinned conservative static slice (`src/static_slice.rs`);
  - an exhaustive single-byte sweep, as the recall reference.
- **16 controlled cases** on 15 fixture binaries (11 C sources; x86-64 -O0 and -O2,
  AArch64 -O0), in `src/eval.rs`, `testdata/src/`.
- The **harness** `examples/csic_eval.rs` and a **native check**
  `testdata/native/verify_native.py`.

There is **no CLI subcommand**. A `fission_cli causal-trace` would make the
production CLI depend on this crate, which the isolation rule forbids. The
harness example is the entry point instead.

## 2. API inventory (P0)

Every finding was checked by reading the code and running the fixtures.

### Time-travel debugging (`fission-ttd`, `Emulator::ttd_*`)

**What a snapshot holds.** `TTDRecorder` stores one `ExecutionSnapshot` every
`ttd_snapshot_interval` instructions. Each holds:
- the maximal registers, by name, with vector registers as bytes;
- the memory and shadow deltas written since the previous snapshot.

**How `ttd_seek(step)` works.** It restores the nearest snapshot at or before the
target by undoing or redoing deltas, then re-executes forward to the target.

**What it does not restore.** The emulator's OS-side state:
- the VFS (including stdin's cursor);
- `heap_cursor`, `fs_base`, signals and `exit_code`;
- the Linux task scheduler.

`core.rs` says so itself ("the OS layer's own state ... is not in a
snapshot"). A seek is exact only while replaying the recorded timeline.

**One timeline only.** Recording after a rewind truncates the future
(`prepare_slot`, `ttd_discard_future`).

So TTD is a single-timeline debugger, not a branching counterfactual store. CSIC
does not use it.

### Snapshot, restore and fork

- `EmulatorSnapshot::capture/restore_into` clones `MachineState` and the PC. It
  carries none of the fields above, and always flushes the JIT cache.
- **There is no copy-on-write fork API, and `Emulator` is not `Clone`.** It holds
  `Box<dyn OsEnvironment>` with a mutex-guarded scheduler, and the JIT.
- **The COW primitive does exist one level down.** `AddressSpace.pages` is an
  `im::HashMap` of `Arc<Vec<u8>>` pages, so cloning `MachineState` shares every
  page until it is written. A clone costs:
  - the 32 KiB host register file (which *is* the register file: clearing it
    zeroes every register);
  - the page-map bookkeeping.
- **Built for P1:** `Checkpoint`, which is `MachineState` plus 15 emulator fields.
  - The VFS is shared between checkpoints with the same file state, because the
    VFS carries the program image three times as seeded paths.
  - A checkpoint is restored into a pooled emulator, and the JIT cache is kept
    unless the recording wrote an executable page.
  - The OS layer's private state is the pooled emulator's own. This is the one
    unsoundness a checkpoint could carry, which is why each checkpoint must pass
    a null-intervention replay (same Φ, same exit status and stdout) before it
    is trusted. *Measured: 595 checkpoints over the 16 cases; 0 failed
    validation.* A restore costs a median of 43 µs per case.
- **`Emulator::new` is expensive.** `register_map_for_load_spec` and
  `userop_map_for_load_spec` each re-read and re-parse the compiled `.sla`, with no
  cache. *Measured: a median of 845 ms per emulator (369–2163 ms on a loaded
  machine).* A fork therefore cannot be "build a new
  emulator"; that would dominate every experiment.

### Replay determinism

- **Inputs and time.** Stdin is a seeded mock, and host stdin is off by default.
  The environment syscalls are deterministic stubs:
  - `time`, `clock_gettime` and `gettimeofday` return 1 700 000 000;
  - `getrandom` returns a fixed pattern;
  - `get*id` returns 1000;
  - `uname` is fixed;
  - `RDTSC` is a counter.
- **That determinism hides native non-determinism.** A replay always agrees with
  itself, even when the native program would not. CSIC therefore classifies
  these sources by syscall name (ISA-independent) and refuses to attribute when
  Φ depends on one (section 3).
- **Engines agree on end state.** JIT and interpreter end states (exit status and
  stdout) agreed on every recorded fixture run, and two fresh JIT replays always
  agreed.

**`max_inst` is not exact under the JIT.**
- Of 50 sampled budgets, 8 to 13 per fixture retired `n + 1` instructions, on
  x86-64 and AArch64. One sample: `with_max_inst(Some(129))` gave 130.
- The interpreter is exact.
- CSIC therefore positions with the interpreter and finishes replays on the JIT.
- This contradicts the comment on `run_instruction`, and is worth an emulator
  issue.

**Memory faults are not delivered to the guest as `SIGSEGV`.** The run returns
`Err`.
- The JIT's message names the guest PC; the interpreter's does not.
- CSIC takes the fault PC from single-stepping.
- A program that handles `SIGSEGV` would behave differently natively.

### Taint (`fission-emulator::taint`)

Fission's taint engine has two parts:
- per-byte provenance with interned sets;
- bounded implicit-flow (control) scopes from static reconvergence.

It is reusable, so the taint baseline uses it rather than a new one. Two defects
came up, both reproducible with this crate's fixtures:

1. **The interpreter panics on a store through a tainted pointer.**
   - The panic is in `memory_mixin.rs:71`: `unwrap` on a `None` solver node.
     `handle_symbolic_write` receives a taint set id as if it were a solver node.
   - Fixtures that hit it: `rfc_auth`, `real_alias`, `crash_null`.
2. **The JIT's taint loses direct-data provenance through a memory store that the
   interpreter keeps.**
   - On `false_alias_ptr`, `cfg.flag = req[0] & 1` gives `{in[0]}` under the
     interpreter and `{}` under the JIT.
   - The JIT does propagate pointer taint, which the interpreter cannot reach
     because of defect 1.

The baseline runs the interpreter first and falls back to the JIT when the
interpreter panics. The engine used is recorded per case.

### Solver (for later joint-cause encoding)

`fission-solver` provides:
- pure-Rust QF_BV plus arrays;
- CDCL with conflict budgets;
- an explicit `Unknown`;
- a z3 differential test.

`ShadowMode::Symbolic` already builds a `SymExpr` per operation, makes stdin bytes
8-bit variables, and records `SymBranch` events with condition nodes. That is
enough to encode "Φ's guard under a contingency" over input bytes for P2. P1 does
not use the solver.

## 3. Contract v1

**Φ (`SinkSpec`)** is one of:
- `FaultAt { pc }`;
- `CallTo { target, args: [Option<u64>] }`, read through the language's
  calling convention;
- `BranchOutcome { pc, occurrence, taken }`;
- `MemoryValue { addr, len, Equals | NonZero, at: Exit | Step | PcHit }`.

Recording and every replay evaluate Φ through the same `SinkMonitor`. Φ is
three-valued: `Holds`, `DoesNotHold`, or `Undecided` (instruction budget).

**Interventions.** `do(location := value)` happens right before the instruction
at `step`, after exactly `step` instructions have retired. A location is a named
register from the language's register map, or a RAM range.

An intervention is invalid (`InvalidReason`) if:
- the register is unknown;
- it targets the program counter;
- the memory is unmapped;
- the memory is executable (that would be patching code);
- the value is the wrong width;
- the value equals the recorded one (a no-op);
- it is at or after the sink step.

The stack pointer and program counter are never *generated* as candidates. They
change frame or control structure, not a value.

**Fingerprint (`ReplayFingerprint`)** records:
- the contract version and the binary's BLAKE3;
- the language id;
- the input stream's BLAKE3 and length;
- argv and envp;
- the BLAKE3 of the initial register file and every mapped page;
- every environment source the run consumed, with its status;
- the engine.

**Environment sources** are classified by syscall name:
- non-deterministic natively: time, clock, entropy, pids, `set_tid_address`,
  host file reads;
- host-stable: uid, uname.

A non-deterministic source is:
- `InSinkSlice` if the slice of Φ reaches its output;
- `PerturbationChangesSink` if a counterfactual on its outputs flips Φ;
- otherwise `OutsideSliceStable`, which is flagged but not refused.

Either of the first two refuses the whole case.

**Verdicts:**
- `CounterfactualWitness`: a one-location flip, confirmed by a fresh replay from
  the initial state and by the other engine.
- `NotACause`: holds for the listed values, in this experiment.
- `InvalidIntervention`.
- `NonDeterministic`: the fork and the fresh replay disagree, or the case was
  refused.
- `BudgetBounded`.
- `Unsupported`.

A case-level `JointStatus` adds `JointWitness`, `NoWitnessWithinBudget` (which
is explicitly *not* "no cause"), `NotNeeded` and `Refused`.

**What a witness is not.** It is not Halpern–Pearl actual causation. No
contingency set was searched, AC2/AC3 were not checked, and no minimality is
claimed. A `JointWitness` means only that no single tried location sufficed and
this pair did.

## 4. Runner

1. **Record.** Single-step the interpreter from the initial state, run to the
   end, and decide Φ.
2. **Gates.** Two fresh JIT replays must agree with each other and with the
   recording. Φ must be decided. Then come the environment checks above.
3. **Candidates.**
   - The backward dynamic slice from Φ's criterion. Data is exact, because
     memory addresses are concrete. Control dependence uses each activation's
     dynamic control parent from static postdominators; a call opens a new
     activation whose parent is the call.
   - Potential dependence: a branch executed between a value's last definition
     and its use counts when the arm it did not take could have redefined the
     value. That arm is the static region up to the branch's immediate
     postdominator, and "could have redefined" means a store or call, or a
     write to the same register bytes.
   - Ordered as: input bytes by offset (intervened right after the read that
     delivered them), then every location a slice step read, nearest the sink
     first.
4. **Values per candidate**, deterministic, at most 6:
   - low-bit flip, 0 (or 1), top-bit flip, complement, +1, −1;
   - then the comparison constants seen in slice steps.
5. **Oracle.**
   - Restore the closest checkpoint at or before the step (thinned to one per 16
     steps) and replay the rest of the way on the interpreter.
   - Intervene, then finish on the JIT with breakpoints on the sink's addresses.
     A fault sink single-steps instead.
6. **Verification.** Every flip is replayed from step 0 with interpreter
   continuation. The first verification of a case uses a brand-new emulator;
   later ones use a second pooled emulator that holds only the initial
   checkpoint. The JIT end state must match too.
7. **Joint search.** When no single witness exists, for each input-byte
   candidate:
   - record the counterfactual run with its first tried value;
   - slice that run;
   - pair the byte with every input byte either slice reaches (dual slicing).
   The search is bounded (64 pairs), and every hit is verified fresh.

## 5. Results

Run with `cargo run --profile quick-release -p fission-causal --example csic_eval -- --out report.json`
and the default `Config`:
- at most 600 candidates and 6 values each;
- at most 4 000 oracle executions and 64 pairs;
- a checkpoint every 16 steps or more;
- control and potential dependence on.

Each case ran in its own process. The machine was shared with two DecBench
sweeps and another build, so absolute times are pessimistic and noisy; the counts
are deterministic.

### Explanations against ground truth (input bytes; TP/FP/FN)

| Case | Property | Truth | Static slice | Dynamic slice | Taint (data) | Taint (data+ctrl) | CSIC | Exhaustive |
|---|---|---|---|---|---|---|---|---|
| `rfc_auth_x64_O0` | RFC case | {3} | 32 bytes 1/31/0 | {3} 1/0/0 | {} 0/0/1 | {} 0/0/1 | {3} 1/0/0 | {3} |
| `rfc_auth_x64_O2` | RFC case at -O2 (the branch may become data flow) | {3} | 32 bytes 1/31/0 | {3} 1/0/0 | {} 0/0/1 | {} 0/0/1 | {3} 1/0/0 | {3} |
| `rfc_auth_a64_O0` | RFC case on AArch64 (ISA-agnostic path) | {3} | 32 bytes 1/31/0 | {3} 1/0/0 | {} 0/0/1 | {3} 1/0/0 | {3} 1/0/0 | {3} |
| `invalid_interventions` | contract violations are refused; valid positive/negative controls judged | {3} | 32 bytes 1/31/0 | {3} 1/0/0 | {} 0/0/1 | {} 0/0/1 | {3} 1/0/0 | {3} |
| `implicit_call_arg` | implicit flow | {5} | 16 bytes 1/15/0 | {5} 1/0/0 | {} 0/0/1 | {5} 1/0/0 | {5} 1/0/0 | {5} |
| `false_alias_ptr` | false alias | {0} | 16 bytes 1/15/0 | {0} 1/0/0 | {0} 1/0/0 | {0} 1/0/0 | {0} 1/0/0 | {0} |
| `real_alias` | real alias | {2,9} | 16 bytes 2/14/0 | {2,9} 2/0/0 | {2,9} 2/0/0 | {2,9} 2/0/0 | {2,9} 2/0/0 | {2,9} |
| `joint_or` | overdetermined grant (A || B, both true) | joint{0,1} | 16 bytes 2/14/0 | {0} 1/0/1 | {} 0/0/2 | {} 0/0/2 | {0,1} + pair{0,1} 2/0/0 | {} |
| `joint_and` | conjunctive grant (K && Y) | {0,1} | 16 bytes 2/14/0 | {0,1} 2/0/0 | {} 0/0/2 | {} 0/0/2 | {0,1} 2/0/0 | {0,1} |
| `nondet_time` | grant depends on the wall clock | refuse | 16 bytes 1/15/0 | {0} 1/0/0 | {} 0/0/1 | {} 0/0/1 | refused 0/0/1 | {0} |
| `nondet_random` | grant depends on getrandom | refuse | 16 bytes 1/15/0 | {0} 1/0/0 | {} 0/0/1 | {} 0/0/1 | refused 0/0/1 | {0} |
| `crash_null_x64` | crash | {0} | 8 bytes 1/7/0 | {0,1} 1/1/0 | {0,1} 1/1/0 | {0,1} 1/1/0 | {0} 1/0/0 | {0} |
| `crash_null_a64` | the crash case on AArch64 | {0} | 8 bytes 1/7/0 | {0,1} 1/1/0 | {0,1} 1/1/0 | {0,1} 1/1/0 | {0} 1/0/0 | {0} |
| `password_x64` | authorization sink | {0,1,2,3} | 16 bytes 4/12/0 | {0,1,2,3} 4/0/0 | {} 0/0/4 | {} 0/0/4 | {0,1,2,3} 4/0/0 | {0,1,2,3} |
| `password_a64` | the authorization case on AArch64 | {0,1,2,3} | 16 bytes 4/12/0 | {0,1,2,3} 4/0/0 | {} 0/0/4 | {} 0/0/4 | {0,1,2,3} 4/0/0 | {0,1,2,3} |
| `branch_len` | branch-outcome sink with direct data flow (positive control for taint) | {0} | 16 bytes 1/15/0 | {0} 1/0/0 | {0} 1/0/0 | {0} 1/0/0 | {0} 1/0/0 | {0} |

**Legend.**
- *Static slice*: the pinned conservative slice defined in
  `src/static_slice.rs`. Its scope is every function the run entered, decoded
  completely. It is interprocedural and context-insensitive, with no points-to
  analysis: an unresolved access may alias all memory. Libc is analysed, not
  modelled. Indirect targets are the observed ones. Its node count is static
  instructions in the slice.
- *Dynamic slice*: the runner's own data, control and potential slice, before
  any intervention.
- *Taint*: Fission's taint engine with one label per input byte. *data* is the
  direct-data projection; *data+ctrl* adds Fission's implicit-flow scopes.
  - An event sink with no value ("the call happened") is read at the branch
    that guards it.
  - The taint engine is the interpreter unless it panics, and then the JIT.
    In spot checks, `rfc_auth`, `real_alias` and `crash_null` used the JIT and
    `false_alias_ptr` used the interpreter. This report does not record the
    engine for each case.
- *CSIC*: input bytes with a verified single-byte witness, plus the members of
  a verified joint pair.
- *Exhaustive*: every delivered byte tried alone with the runner's values; the
  recall reference for single-byte causes.

**Totals over the 14 attributable cases** (the two refusals excluded):

| Method | TP | FP | FN | Precision | Recall |
|---|---|---|---|---|---|
| Static slice | 23 | 249 | 0 | 0.08 | 1.00 |
| Dynamic slice (data + control + potential) | 22 | 2 | 1 | 0.92 | 0.96 |
| Taint, data | 6 | 2 | 17 | 0.75 | 0.26 |
| Taint, data + control | 8 | 2 | 15 | 0.80 | 0.35 |
| **CSIC** | **23** | **0** | **0** | **1.00** | **1.00** |
| Exhaustive single-byte | 21 | 0 | 2 | 1.00 | 0.91 |

**Refusals.** Both non-deterministic cases were refused (`clock_gettime`,
`getrandom` in the sink slice), and no other case was. Every case consumed
`set_tid_address` (musl start-up); it was flagged `OutsideSliceStable` every time.

**Invalid interventions** (`invalid_interventions`):
- Positive control: `req_buf[3] := 0` is a witness.
- Negative control: `req_buf[9] := 0` is `NotACause`.
- Each of the seven violations got its own reason: `ProgramCounter`,
  `UnknownRegister`, `Unmapped`, `CodePage`, `WidthMismatch`, `NoOp`,
  `NotBeforeSink`.

### What each case shows

- **RFC case** (`rfc_auth_*`, `invalid_interventions`). `req_buf[3]` is the only
  witness on x86-64 -O0, x86-64 -O2 and AArch64 -O0.
  - The static slice blames all 32 bytes. The logged bytes 8..11 reach
    `session.is_admin` through the variable-index `log_buf` store, which it must
    treat as aliasing, and through `MemAny` everywhere after that.
  - Data taint misses `req_buf[3]` (the stored value is the constant 1).
  - Fission's control taint catches it on AArch64 but not on x86-64. That
    divergence follows the JIT store-provenance defect in section 2.
- **Implicit flow** (`implicit_call_arg`). Data taint misses `req[5]`; control
  taint and CSIC find it.
- **False alias** (`false_alias_ptr`). Every dynamic method is right. The static
  slice blames 15 non-causes.
- **Real alias** (`real_alias`). `req[2]` (store offset) and `req[9]` (stored
  value) are both witnesses. The JIT taint finds both because it propagates
  pointer taint.
- **Joint cause** (`joint_or`).
  - No single byte is a witness, and the exhaustive sweep agrees.
  - The recorded run never reads `req[1]` (`||` short-circuits). CSIC finds it
    from the counterfactual run's slice and reports a verified `JointWitness`
    `{req[0], req[1]}`, not "no cause".
  - Contrast `joint_and`: two single witnesses and no pair search.
- **Crash** (`crash_null_*`).
  - The fault is caused by the slot selector `req[0]`, through an address
    dependence.
  - The dynamic slice and both taints also blame the stored value `req[1]`,
    because it flows into the faulting store. Intervening on it never prevents
    the fault, so CSIC drops it.
- **Authorization sink** (`password_*`).
  - With plain data and control slicing, the slice holds no input byte at all.
    The `ok = 0` store that depends on the compared bytes never executed: an
    omission dependence. The exhaustive sweep shows all four bytes are causes.
  - Following *potential* dependence fixes it: a branch counts if its untaken
    arm could have redefined a used value (relevant slicing).
  - Both taints still miss all four.
  - `tests/csic_cases.rs::a_check_whose_failing_arm_never_ran_needs_potential_dependence`
    has the positive and negative controls.
- **Branch outcome** (`branch_len`). Every dynamic method agrees.

### Native execution

`testdata/native/verify_native.py` runs the fixtures natively in Docker:
- AArch64 runs on the host CPU in the Linux VM;
- x86-64 runs under Docker's x86-64 translation, which is independent of
  Fission's SLEIGH lift.

It checks:
- the recorded input;
- every CSIC input-byte witness (Φ must flip natively);
- both members of each joint pair (each alone must not flip Φ; together they
  must);
- **every byte CSIC did not name**, with the runner's first two alternatives
  (Φ must not flip natively). This includes the bytes each baseline blames.

For the two refused cases, it repeats the recorded input 12 times and requires
both outcomes to occur natively.

**Result: 559 native checks over the 16 cases, all agree with the emulator replay
and the CSIC verdicts.** This covers:
- every input-level witness;
- the joint pair and its members;
- the bytes each baseline wrongly blamed: `req_buf[8..11]` in the RFC case, the
  stored value `req[1]` in the crash cases, and every static-slice false
  positive.

Both refused programs showed both outcomes natively within 12 runs, so the
non-determinism the emulator's stubs hide is real.

The internal-location witnesses (registers, stack slots) cannot be checked
natively. Their evidence is the fresh replay from step 0 plus the JIT and
interpreter agreeing on the end state.

### Cost

| Case | Steps | SS instrs | DS steps | CSIC candidates | witnesses (input/internal) | oracles | fresh verifs | checkpoints (invalid) | ckpt capture ms | restore µs | replay-to-step µs | oracle ms | fresh emulator ms | peak RSS MiB | CSIC total s | exhaustive oracles |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `rfc_auth_x64_O0` | 808 | 378 | 507 | 600 | 1/245 | 2589 | 246 | 38 (0) | 36 | 52 | 653 | 23.0 | 1243 | 462 | 67.9 | 156 |
| `rfc_auth_x64_O2` | 722 | 358 | 464 | 600 | 1/253 | 2518 | 254 | 36 (0) | 69 | 21 | 598 | 4.9 | 1074 | 501 | 18.6 | 156 |
| `rfc_auth_a64_O0` | 840 | 500 | 432 | 542 | 1/271 | 2433 | 272 | 40 (0) | 44 | 311 | 672 | 7.6 | 2163 | 497 | 28.5 | 156 |
| `invalid_interventions` | 808 | 378 | 507 | 600 | 1/245 | 2593 | 247 | 39 (0) | 83 | 57 | 418 | 15.0 | 813 | 502 | 45.8 | 156 |
| `implicit_call_arg` | 733 | 332 | 469 | 600 | 1/253 | 2521 | 254 | 35 (0) | 34 | 22 | 349 | 11.2 | 678 | 509 | 31.7 | 76 |
| `false_alias_ptr` | 808 | 382 | 506 | 600 | 1/248 | 2570 | 249 | 39 (0) | 21 | 21 | 257 | 7.9 | 369 | 561 | 23.1 | 76 |
| `real_alias` | 719 | 366 | 468 | 600 | 2/255 | 2517 | 257 | 36 (0) | 17 | 51 | 892 | 21.7 | 457 | 519 | 57.9 | 74 |
| `joint_or` | 718 | 329 | 468 | 600 | 0/249 | 2539 | 250 | 35 (0) | 18 | 39 | 651 | 18.4 | 896 | 507 | 53.6 | 80 |
| `joint_and` | 721 | 329 | 471 | 600 | 2/253 | 2514 | 255 | 36 (0) | 42 | 53 | 855 | 20.1 | 814 | 552 | 59.5 | 72 |
| `nondet_time` | 906 | 618 | 651 | 0 | 0/0 | 52 | 0 | 39 (0) | 92 | 47 | 1670 | 6.6 | 0 | 526 | 7.9 | 76 |
| `nondet_random` | 757 | 349 | 505 | 0 | 0/0 | 50 | 0 | 37 (0) | 125 | 22 | 2973 | 6.0 | 0 | 510 | 6.4 | 76 |
| `crash_null_x64` | 602 | 294 | 470 | 600 | 1/254 | 2530 | 255 | 36 (0) | 53 | 63 | 845 | 45.9 | 876 | 518 | 123.6 | 37 |
| `crash_null_a64` | 577 | 424 | 394 | 515 | 1/272 | 2266 | 273 | 36 (0) | 9 | 36 | 245 | 7.3 | 588 | 737 | 23.1 | 36 |
| `password_x64` | 779 | 342 | 517 | 600 | 4/266 | 2530 | 270 | 37 (0) | 46 | 58 | 550 | 16.6 | 912 | 503 | 49.8 | 64 |
| `password_a64` | 797 | 458 | 450 | 585 | 4/327 | 2503 | 331 | 40 (0) | 45 | 24 | 188 | 3.1 | 380 | 737 | 10.8 | 64 |
| `branch_len` | 718 | 334 | 471 | 600 | 1/252 | 2522 | 253 | 36 (0) | 91 | 33 | 410 | 11.6 | 1556 | 506 | 33.5 | 77 |

**Where the cost goes.**
- About 2 500 oracle executions per case (35 247 in total). Almost all of them
  test internal locations: registers, flags and stack slots read by slice
  steps.
- Those produce 3 664 internal witnesses. They are true counterfactuals but not
  explanations.
- The input-byte answer needed 36 to 156 oracle executions per case in the
  exhaustive sweep (1 432 in total).
- Where `candidates_generated` exceeded 600, the tail went untested. Input bytes
  come first, so the scored explanations are unaffected.
- 828 candidate verdicts are `BudgetBounded`. A replay ran past its instruction
  budget, typically because a corrupted loop counter or return address made
  the program loop.

**Fork and replay costs** (median of per-case means):
- restoring a checkpoint: 43 µs;
- replaying to the exact step: 0.2–3 ms;
- one oracle on the JIT: 3–46 ms;
- building a fresh emulator: 845 ms. That is 20 000 times a restore, which is
  why the pooled-worker design matters.

**Peak RSS: 462–737 MiB per case.** It is dominated by emulator instances: the
recorder, the worker, the verifier, the taint run and the exhaustive sweep each
hold the parsed Sleigh tables. Checkpoints are not the main cost: about 37 per
case, each a 32 KiB register file plus the pages written since.


## 6. Limitations: what the runner cannot do yet

- **No actual-cause semantics.** There is no contingency search beyond one
  bounded pair, no AC2/AC3 check and no minimality. A `JointWitness` is the first
  verified pair found, not a minimal set.
- **Candidate recall is the slice's recall.** A byte the recorded run never read
  is never a single candidate. `joint_or` shows the gap: `req[1]` enters only
  through the counterfactual run's slice. The exhaustive column measures what
  the generator misses on each case.
- **The value set is a heuristic.** `NotACause` holds for the tried values only.
  A cause that flips Φ for one specific value (an equality the recorded run did
  not satisfy) is found only if that constant appears in a compared slice step.
- **Static, single-threaded Linux ELF only.**
  - A step that does not retire exactly one instruction (an HLE import stub in a
    dynamically linked binary) is rejected with `Unsupported` during recording.
  - Guest threads and signals are not modelled in checkpoints. The
    null-intervention validation would catch a divergence, but nothing was
    tested with them.
  - ARM32/Thumb is untested: the recorder decodes without the emulator's
    context override.
- **Checkpoints do not carry the OS layer's private state.** A checkpoint holds
  `MachineState` and 15 emulator fields. Soundness rests on the per-checkpoint
  null-intervention replay, not on completeness.
- **Faults.** The emulator returns an error instead of delivering `SIGSEGV`, so a
  fault sink is "the run stopped with a memory fault at this PC". A guest signal
  handler is not modelled.
- **Environment.** Only syscalls are classified. A non-deterministic *userop*
  other than `RDTSC` (`RDRAND`, `RDSEED`, `CPUID` variance) is not.
- **Cost.** Every slice step's reads are candidates, so a case tests about 300
  time-indexed locations and spends about 1.5k oracle executions in about 45 s.
  Most witnesses are internal registers and stack slots, which are true
  counterfactuals but not explanations. Input-byte granularity is what is scored.
- **The static baseline is minimal on purpose.** No points-to analysis, context
  insensitive, libc analysed rather than modelled. A slicer with value ranges and
  libc models would separate direct accesses (for example `branch_len`'s
  `req[0]`). P3 should compare against one before drawing conclusions about static
  slicing in general.
- **Native evidence covers input-level verdicts only.** Internal-location
  witnesses (registers, stack slots) cannot be replayed natively. Their evidence
  is the fresh replay and the engine cross-check, which share the SLEIGH lift
  (ADR 0017).


## 7. Recommendation for P2/P3

CSIC should **continue**, but it should change what it spends oracles on before
it grows. The numbers that drive that:

1. **Precision is the point, and it held.**
   - CSIC: 23 TP, 0 FP, 0 FN on 14 attributable cases, both refusals correct,
     and every input-level verdict confirmed natively.
   - The best non-interventional baseline, the runner's own dynamic slice, still
     had 2 FP (crash) and needed potential dependence to avoid 8 FN (password).
   - Taint reached a recall of 0.35. The static slice reached a precision of
     0.08.
   - The fixtures were built to separate these methods, so this is a mechanism
     check, not an accuracy estimate.
2. **Cost is in the wrong place.**
   - 99% of oracle executions (about 2 500 a case) test internal locations,
     against 36–156 for the input-level answer.
   - **P2 change:** make input bytes (and later, named program variables) the
     primary candidates, ranked by the slice. Test internal locations only on
     request, or to localise a mechanism after an input-level witness.
   - Expected effect, from this run: about 15–25x fewer oracle executions
     (35 247 down to roughly 1.5–2 k on the same cases) with the same scored
     explanations.
3. **Joint causes need a real search.** One bounded pair is enough for the
   fixtures, not for HP. P2 should:
   - encode Φ's guard over the input bytes with the emulator's
     `ShadowMode::Symbolic` and `fission-solver`;
   - ask for contingencies (keep W fixed, flip X) inside a stated budget;
   - replay every solver model in the emulator before using it, as the RFC asks;
   - test AC2/AC3 within the budget, and only then use a minimality label.
4. **Fix the emulator issues this found**, as separate emulator PRs before
   anything depends on them:
   - `Emulator::new` re-parses the `.sla` twice: a median of 845 ms per
     emulator, about 20 000x a checkpoint restore.
   - JIT `max_inst` overruns by one instruction in 16–26% of sampled budgets.
   - The interpreter's taint panics on a store through a tainted pointer.
   - The JIT's taint loses direct-data provenance through a memory store.
   - A first-class `Emulator::checkpoint/restore` (this crate's `Checkpoint`) would
     let the TTD and the debugger share it.
5. **Held-out evaluation (P3).**
   - Run CSIC with input-first candidates on `~/fission-benchmark/dir-verify/binaries`.
     These are real GNU programs with real I/O, which will exercise the refusal
     path harder.
   - Run a second, independently written fixture set, with native execution as
     the check.
   - Compare against a stronger static slicer (points-to, value ranges, libc
     models) and a dual-slicing baseline before any Adopt/Redesign/Reject ADR.

**Suggested stop triggers for P3:**
- any native disagreement with a `CounterfactualWitness`;
- more than 10% `BudgetBounded` among input-level candidates;
- a refusal rate above 50% on real programs, which would make the determinism
  gate too blunt to be useful.



## 8. P2: input-first candidates and a solver-backed joint search

### 8.1 Setup on main

`feat/csic-p0-p1` was merged with `origin/main`, which brought the emulator fixes P1 asked for (#1109–#1113). The crate was then adapted to them:

- **Faults are a guest signal (#1112).** The recorder and both observers now read a fault from the emulator itself, through `metrics.memory_faults` and `last_fault`, instead of parsing an `Err`.
  - An unhandled fault ends the run with `exit_signal = 11`. `last_fault.pc` names the instruction, which did not retire.
  - A fault delivered to a handler is folded into the handler's first step. The faulting instruction's uses join that step, and the step opens an activation of its own, as a call would.
  - Checkpoints carry `exit_signal` and `last_fault`. A `FaultAt` sink no longer single-steps on the fast path.
- **Taint is reliable in both engines (#1111).** The taint baseline drops its JIT fallback and runs on the interpreter.
- **Construction is cheap (#1113).** The case harness was left as it was.

The **P1 evaluation was re-run unchanged** on the merged tree. On every case CSIC's explanations, TP/FP/FN and verdicts are identical to P1. Oracle counts differ by at most one per case: 35 246 in total, against 35 247 in P1. One baseline column moved: Fission's data+control taint now finds `req_buf[3]` in the x86-64 RFC cases (8/2/15 became 11/2/12), as #1111 predicted.

### 8.2 Two more emulator defects (PR #1202)

The joint search asks the solver about the symbolic branches an interpreter replay records. On the first fixture it recorded none. Both causes are in the interpreter's `ShadowMode::Symbolic` path:

1. **In-place ops lost their expression.**
   - The interpreter runs a p-code op concretely and builds its expression afterwards, and a concrete write clears the shadow it covers.
   - An op whose output overlaps an input (`ZEXT RAX <- EAX`, which every x86-64 32-bit register write lifts to) read its own cleared output.
   - The binop and load paths also read operand values after the op had overwritten them.
2. **`STORE` passed its callback the wrong arguments.** The pointer's varnode went where the value's belongs, and the sizes were swapped. Every `-O0` stack spill dropped its expression.

The fix is its own PR against main, **#1202**: inputs are snapshotted before the step, as the taint path already does, and the callback gets its arguments in the JIT's order. It has four tests:
- a positive control that passed before the fix;
- three regressions that failed before it.

The full emulator suite passes (414 tests). The CSIC branch merges #1202, so PR #1106's diff shows those two files until #1202 lands.

### 8.3 What changed in the runner (contract `csic/v2`)

**Candidate tiers.**
1. The input bytes the slice reaches, ranked by the slice step that last read them (nearest the sink first). Then the named variables (sized data symbols, at most 32) the slice reads.
2. Delivered input bytes outside the slice, tried only if tier 1 has no input-byte witness. This tier means the explanation's recall is no longer the slice's recall.
3. Internal registers and stack slots, P1's whole candidate set. They are tried only with `InternalPolicy::Always`, or when no input-level witness exists, whether single or joint. Their oracle executions are counted separately (`oracle_executions_internal`).

**Joint search.** It runs when no single input byte is a witness. It is a generational search (SAGE-style) over the input bytes:
- **Replay.** A symbolic replay runs on the interpreter with stdin bytes as 8-bit solver variables. Variables are renamed to canonical, offset-derived ids before any query, so models do not depend on the global variable counter.
- **Queries.** From a path, the search asks to negate Φ itself (when Φ's operand is symbolic) and then each symbolic branch, nearest the sink first, keeping the branches before it.
  - Each query states that at most k of the free bytes differ from the recorded input, with k ascending from the node's own change count, so the smallest change comes first.
  - The other bytes are pinned to their recorded values. Each query uses a fresh solver with a conflict budget.
- **Every model is replayed.** Each model is checked in order:
  1. A model whose replay does not take its predicted path is **rejected** and logged.
  2. A model that flips Φ must be confirmed by an intervention replay (fork, JIT), a fresh replay from step 0 and the JIT digest.
  3. A model that leaves Φ alone has its own path searched past the negated branch.
- **Blind spots end in Unknown.** If any model was rejected, or the recording has input-dependent branches the symbolic replay never saw, the search ends **Unknown**. Running out of paths proves nothing then. Input-dependent branches are found by a forward pass over the recorded uses and definitions.
- **Statuses.** `JointStatus::SolverUnknown` is reported explicitly and is never "no cause". `NoWitnessWithinBudget{exhausted}` says whether paths or budget ran out.
- **Fallback.** P1's pair search runs only if the solver finds nothing.
- **A one-byte model** that the value set missed becomes a single witness with origin `SolverModel`.

**Bounded minimality (`Minimality::MinimalWithinBudget`).** For a joint witness of k bytes, each (k−1)-subset is searched with the same search. That search may leave any byte at its recorded value, so it covers every smaller subset.
- The verdict is given only if every subset search runs out of paths, with no Unknown, no rejected model and no budget hit.
- A smaller witness replaces the set, and the check restarts.
- The bound is written into the verdict:
  - input bytes only, with the other bytes at their recorded values;
  - 8 generations, at most 4 changed bytes and 100 000 SAT conflicts per query;
  - complete only over what the interpreter's symbolic shadow models.
- **This is not Halpern–Pearl minimality.** AC2(a/b) allows any contingency over all variables, internal state included; this check covers input bytes at their actual values only.

**Contingencies (`ContingencyWitness`).** For each member X of a joint witness, the run checks:
- that do(the other members) alone leaves Φ at its recorded value, on both a fork and a fresh replay;
- that adding X flips it.

That makes X a counterfactual witness under a bounded contingency (|W| ≤ k−1), at the model's values.

**Requested sets.** `Case.requested_sets` are judged as one experiment each, on a fork and a fresh replay. The new cases use them for their positive and adversarial controls.

### 8.4 Oracle executions, before and after (the 16 P1 cases)

Before is the P1 runner re-run on merged main; after is P2's default `Config` (internal locations `Fallback`). Counts are deterministic.

| Case | before | after | ratio | tested (tier 1 / 2 / internal) | CSIC explanation (TP/FP/FN), before → after |
|---|---|---|---|---|---|
| `rfc_auth_x64_O0` | 2589 | 34 | 76x | 4 / 0 / 0 | {3} 1/0/0 → same |
| `rfc_auth_x64_O2` | 2518 | 34 | 74x | 4 / 0 / 0 | {3} 1/0/0 → same |
| `rfc_auth_a64_O0` | 2433 | 19 | 128x | 3 / 0 / 0 | {3} 1/0/0 → same |
| `invalid_interventions` | 2593 | 37 | 70x | 4 / 0 / 0 | {3} 1/0/0 → same |
| `implicit_call_arg` | 2521 | 34 | 74x | 4 / 0 / 0 | {5} 1/0/0 → same |
| `false_alias_ptr` | 2570 | 34 | 76x | 4 / 0 / 0 | {0} 1/0/0 → same |
| `real_alias` | 2517 | 38 | 66x | 5 / 0 / 0 | {2,9} 2/0/0 → same |
| `joint_or` | 2539 | 139 | 18x | 4 / 15 / 0 | joint {0,1} 2/0/0 → same (now by the solver) |
| `joint_and` | 2514 | 36 | 70x | 5 / 0 / 0 | {0,1} 2/0/0 → same |
| `nondet_time` | 52 | 19 | 3x | refused | refused → refused |
| `nondet_random` | 50 | 18 | 3x | refused | refused → refused |
| `crash_null_x64` | 2529 | 47 | 54x | 6 / 0 / 0 | {0} 1/0/0 → same |
| `crash_null_a64` | 2266 | 32 | 71x | 5 / 0 / 0 | {0} 1/0/0 → same |
| `password_x64` | 2530 | 43 | 59x | 8 / 0 / 0 | {0..3} 4/0/0 → same |
| `password_a64` | 2503 | 28 | 89x | 7 / 0 / 0 | {0..3} 4/0/0 → same |
| `branch_len` | 2522 | 35 | 72x | 4 / 0 / 0 | {0} 1/0/0 → same |
| **total** | **35 246** | **627** | **56x** | | **23/0/0 → 23/0/0, both refusals kept** |

**The reduction is 56x, against the 15–25x the P1 report estimated.**
- The estimate assumed every delivered byte would be tried, as the exhaustive sweep does (1 431 oracles on the same cases).
- In practice the slice ranks 3–8 primary candidates per case, the first input witness ends the search, and only about 5 checkpoints a case need capturing and validating, instead of about 37.
- No case reached the internal tier. `oracle_executions_internal` is 0 everywhere.
- `tests/csic_cases.rs::input_first_spends_no_oracles_on_internal_locations_once_an_input_witness_exists` keeps the positive control. With `InternalPolicy::Always`, internal locations are tested and cost oracles, and the explanation is the same.

**Named variables are mostly libc's in a static binary.**
- `__libc+16` is a witness in every case that reaches it: corrupting it changes how the program runs. `main_tls` and `__default_stacksize` are not.
- The one program variable that mattered was `secret+3` in `password_*`, a witness.
- `table+16` (the NULL slot) in the crash and handler cases is `NotACause`, but only for the tried values: every small pointer value faults too.
- None of these are scored. They cost 2–4 oracles per case.

### 8.5 Joint search results, with replay evidence

| Case | Recorded Φ | Joint witness (model) | Found at | Minimality | Contingencies | Solver queries (sat/unsat/unknown) | Symbolic replays | Models (rejected) |
|---|---|---|---|---|---|---|---|---|
| `joint_or` (`A \|\| B`, both true) | grant | {0:'A'→'a', 1:'B'→'R'} | generation 1 | MinimalWithinBudget | 2/2 hold | 6 (3/3/0) | 4 | 3 (0) |
| `joint_and_deny` (`K && Y`, both wrong) | deny | {0:'K', 1:'Y'} | generation 1 | MinimalWithinBudget | 2/2 hold | 6 (3/3/0) | 4 | 3 (0) |
| `and3_x64` (all three wrong) | deny | {0:'K', 1:'E', 2:'Y'} | generation 2 | MinimalWithinBudget (3 subsets) | 3/3 hold | 23 (8/15/0) | 9 | 8 (0) |
| `and3_a64` | deny | {0:'K', 1:'E', 2:'Y'} | generation 2 | MinimalWithinBudget (3 subsets) | 3/3 hold | 23 (8/15/0) | 9 | 8 (0) |
| `carry` (high byte of a stored sum == 1, and 'W') | deny | {1: 160, 2:'W'} | generation 1 | MinimalWithinBudget | 2/2 hold | 6 (3/3/0) | 4 | 3 (0) |
| `carry_impossible` (high byte == 2: unreachable) | deny | none: **SolverUnknown** ("1 solver model did not reproduce") | – | – | – | 1 (1/0/0) | 2 | 1 (**1**) |

**How to read the table.**
- **Replay evidence.** Every model above was replayed in the emulator before it was used (symbolic replays = 1 + distinct models). Each witness was then confirmed three ways: by an intervention fork on the JIT, by a fresh replay from step 0 on the interpreter, and by the JIT end-state digest.
- **`carry_impossible`.**
  - The symbolic shadow tags both bytes of the stored word with the whole sum's expression, so the solver was asked for "sum ≡ 2" where the program reads the high byte. Its model (`req[0] = 160`) gives a low byte of 2, and the replay takes the other way.
  - It is logged as `Rejected { expected: [(…df, false)], observed: [(…df, true)] }`, and the case says Unknown.
  - Without the rejection rule, running out of paths would have read as "no cause". The answer here happens to be no cause, but the search cannot know that.
- **`carry`** shows the same layer's model reproducing when the shadow's coarseness does not matter. The sum is 257, so both bytes happen to fit.
- **The value set alone would find none of these.**
  - The exhaustive single-byte sweep finds nothing in any of the six cases, as designed.
  - P1's pair search tries the low-bit flip and zero, which cannot produce 'K' or 'E'. On the deny cases it has no partners at all: it pairs bytes from the slices, and the slice of a sink that never happened is empty (`carry_impossible`: 0 pairs tried).
- **The XOR and handler cases need no joint search.** Each has single-byte witnesses, so it was not run (`not_needed`).

### 8.6 New cases

Each case has a positive and an adversarial control. They come from the requested sets ("sets") or from the bytes a baseline blames.

| Case | ISA/opt | Truth | Dynamic slice | Taint (data / +ctrl) | Static slice | CSIC | Controls (sets: Φ changed?) |
|---|---|---|---|---|---|---|---|
| `joint_and_deny` | x64 -O0 | joint {0,1} | 0/0/2 | 0/0/2 / 0/0/2 | 2/14/0 | **2/0/0** | {K,Y} yes; {K} no; {Y} no |
| `and3_x64` | x64 -O0 | joint {0,1,2} | 0/0/3 | 0/0/3 / 0/0/3 | 3/13/0 | **3/0/0** | {K,E,Y} yes; {K,E} no; {K,E, decoy req[3]} no |
| `and3_a64` | a64 -O0 | joint {0,1,2} | 0/0/3 | 0/0/3 / 0/0/3 | 3/13/0 | **3/0/0** | same |
| `xor_gate_x64_O0` | x64 -O0 | {0,1} | 2/0/0 | 0/0/2 / 0/0/2 | 2/14/0 | **2/0/0** | flip bit 0 of both: **no** (adversarial); one byte: yes |
| `xor_gate_x64_O2` | x64 -O2 | {0,1} | 2/0/0 | 0/0/2 / 0/0/2 | 2/14/0 | **2/0/0** | same |
| `segv_grant_x64` | x64 -O0 | {0} | 1/**1**/0 | 0/0/1 / 0/0/1 | 0/0/**1** | **1/0/0** | req[0]:=1 yes; stored req[1]:=0 no |
| `segv_grant_a64` | a64 -O0 | {0} | 1/**1**/0 | 0/0/1 / 0/0/1 | 0/0/**1** | **1/0/0** | same |
| `carry` | x64 -O0 | joint {0,1,2} (two minimal witnesses: {0,2}, {1,2}) | 0/0/3 | 0/0/3 / 0/0/3 | 3/13/0 | 2/0/**1** | carry+'W' yes; low byte 1 without carry no |
| `carry_impossible` | x64 -O0 | none | 0/0/0 | 0/0/0 | 0/16/0 | 0/0/0, **SolverUnknown** | low byte 2 (the rejected model) no; largest sum + 'W' no |

**Totals over the 9 new cases.**
- CSIC: **16 TP, 0 FP, 1 FN**.
- Dynamic slice: 6/2/11. The slice of a sink that never happened is empty.
- Taint: 0/0/17.
- Static slice: 15/97/2.
- Exhaustive single-byte sweep: 6/0/11.

**What the new cases show.**
- **The FN is a stated limit.** The search reports *one* minimal joint witness, not all of them. In `carry` it found {1,2} and not {0,2}.
- **XOR.**
  - Both bytes are single witnesses.
  - The adversarial set flips the same bit in both, and Φ stays put. The union of two witnesses is not a joint cause, and the runner never builds joint claims from unions.
  - The decoy `req[2]` is compared on the path and is not blamed.
- **Signal handler.**
  - `grant()` runs only inside the SIGSEGV handler. The recording passes through the delivered fault (`StepRecord::fault`, one per run).
  - The dynamic slice reaches the handler's call through the folded fault. It blames both the selector and the stored value, because both are uses of the faulting store; CSIC keeps only the selector.
  - The static slice misses the cause entirely: it has no edge from a faulting instruction to a signal handler.
  - **Handler-frame limit (#1112):** the emulator's signal frame has no `ucontext_t` (the third argument is NULL) and no `rt_sigreturn`. A handler that returns re-runs the faulting store, natively as well as in the emulator, so the fixture's handler calls `_exit(3)`. A handler that resumes elsewhere through `ucontext` or `siglongjmp` is untested and not expected to work.
  - The x86-64 and AArch64 builds agree.

### 8.7 Native agreement

`testdata/native/verify_native.py` was extended for P2. For every joint witness it now checks:
- the whole model;
- each member alone;
- each set one member smaller (for three or more members);
- each member's contingency alone.

It also checks every solver model the search replayed, rejected ones included (native Φ must equal the emulator replay's Φ), and every requested set. AArch64 runs natively on the host CPU in Docker's Linux VM; x86-64 runs under Docker's `linux/amd64` translation, which is independent of Fission's SLEIGH lift.

**Result: 856 checks over the 25 cases, all agree.** No case failed.

| Kind | Checks |
|---|---|
| recorded input | 23 |
| refusal (both outcomes seen natively over 12 runs) | 2 |
| input-byte single witnesses | 27 |
| bytes CSIC did not name (two alternatives each) | 722 |
| joint witnesses (model, members alone, one-smaller subsets) | 23 |
| contingencies | 12 |
| solver models replayed (including the 1 rejected) | 26 |
| requested sets (positive and adversarial controls) | 21 |

Of these, 313 checks are on the 9 new cases and 543 on the 16 P1 cases. P1 had 559 checks; fewer bytes are now unnamed, because the joint members are named.

The rejected model's input (`carry_impossible`, `req[0] = 160`) does not grant natively. That matches the emulator replay that rejected it, so the rejection was about the symbolic layer, not the emulator.

### 8.8 Limits

- **One joint witness, not all.** The search stops at the first verified one (`carry`: {1,2} found, {0,2} not).
- **The symbolic shadow is coarse.**
  - A multi-byte store tags every byte with the whole value's expression.
  - A widening copy reads its extension bytes from the source's neighbours.
  - Unmodelled ops drop the expression (shifts, and the JIT, which records no branch events).
  - The replay rule and the blind-branch check turn this into Unknown rather than into a wrong claim. They do not make the search complete.
  - The blind-branch check runs on the recorded path only. A child path's unseen branches are not detected.
- **Minimality and contingencies are input-level and bounded.**
  - The other input bytes stay at their recorded values.
  - No internal-state contingency is searched.
  - The bound is the search's: 8 generations, ≤ 4 changed bytes, 100 000 conflicts per query.
  - This is not HP AC2/AC3.
- **Named variables are dominated by libc globals in static binaries**, and their `NotACause` holds only for the value set (`table+16`).
- **The fixtures were written to separate the methods.** The totals are a mechanism check, not an accuracy estimate. Everything is still single-threaded, static Linux ELF.

### 8.9 Recommendation for P3

Continue, with the P1 stop triggers unchanged. The held-out step is now affordable: about 40 oracles a case when an input witness exists, and about 110–140 with a joint search. P3 should:
1. Run input-first CSIC on `~/fission-benchmark/dir-verify/binaries` and on an independently written fixture set, with native execution as the check.
2. Count `SolverUnknown` and `Rejected` models as first-class outcomes. **Add a stop trigger: Unknown on more than 30% of joint searches on real programs** means the symbolic shadow needs work before CSIC can rely on it.
3. Compare against a stronger static slicer (points-to, libc models, signal edges) and a dual-slicing baseline.
4. Enumerate all minimal joint witnesses within the budget. One is reported now.
