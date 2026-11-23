# CSIC P0/P1: contract, fork reality and a one-variable counterfactual runner

**RFC:** #1101, Track B (CSIC, counterfactual machine-state causality)
**Crate:** `crates/fission-causal` (experimental; no production crate depends on it)
**Status:** P0 and P1 done on this crate's own fixtures. Evidence is mechanical plus
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
    is trusted. *Measured: TBD.*
- **`Emulator::new` is expensive.** `register_map_for_load_spec` and
  `userop_map_for_load_spec` each re-read and re-parse the compiled `.sla`, with no
  cache. *Measured: TBD ms per emulator.* A fork therefore cannot be "build a new
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

*Filled in from `examples/csic_eval.rs`; see below.*

RESULTS_PLACEHOLDER

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

RECOMMENDATION_PLACEHOLDER
