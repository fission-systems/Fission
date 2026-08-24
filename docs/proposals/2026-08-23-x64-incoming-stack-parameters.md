# Recover x64 incoming stack parameters from the cspec frame contract

## 1. Baseline Row Anchor

Measured on `main` at `d5b9e36b8` with the official DecBench sample-set type
scorer and a fixed 109-function DWARF-bearing subset. The current result is 6
type-perfect functions and total type distance 1,206.

Five scored x86-64 O2 functions have more than six integer-class parameters.
Fission emits exactly the six register parameters and drops every parameter
transported on the incoming stack:

| sample row | project | source arity | emitted arity | missing slots |
| --- | --- | ---: | ---: | --- |
| `output_diff3_edscript` | diffutils | 7 | 6 | 7 |
| `socket_open2` | gnutls | 10 | 6 | 7-10 |
| `copy_reg` | coreutils | 9 | 6 | 7-9 |
| `ssh_agent_sign` | openssh-portable | 8 | 6 | 7-8 |
| `dopass` | coreutils | 8 | 6 | 7-8 |

The same source functions were measured in the non-scored O0 scale variants
after copying the binaries to `/private/tmp` and stripping all symbols and
DWARF. The defect repeats at O0: `copy_reg` 9 -> 6,
`output_diff3_edscript` 7 -> 6, and `ssh_agent_sign` 8 -> 6. The unstripped
O0 binaries are not evidence because DWARF function hints mask the missing
ABI recovery.

Disassembly supplies a concrete ownership anchor. O2 `copy_reg` moves the
stack pointer by `0x1f8` bytes, then reads incoming slots at
`rsp+0x200`, `rsp+0x208`, and `rsp+0x210`. With the SysV cspec stack base of
8, these are exactly parameter slots 7, 8, and 9. Fission currently renders
them as locals rather than formal parameters. A frame-size-only prototype was
measured and rejected: incomplete prologue scanning reported this function's
frame as `0x20`, and the resulting boundary guess fabricated up to 280 formal
parameters in another sample function.

This is parameter/signature recovery work. No structure or recompilation
improvement is claimed before remeasurement.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder ABI, scalar memory SSA, and stack-slot classification
- [ ] Normalize/type recovery
- [ ] Structuring
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

The cspec and scalar-memory-SSA owners already provide all required facts:

- ordered integer parameter register slots;
- stack parameter base at function entry;
- pointer/slot size;
- an exact entry-SP-relative address for resolvable stack accesses;
- the reaching memory value and whether it is entry input, a store, or a phi.

`AbiState::stack_argument_index` consumes the same cspec facts for outgoing
x64 calls. In contrast, `AbiState::incoming_stack_argument_index` is hard
restricted to x86-32, and `classify_stack_slot_origin` therefore leaves every
x64 incoming stack load as `StackOffset`. The builder never calls
`ensure_incoming_stack_param_binding`, so the final signature stops at the
register count.

The scalar pointer state also exposed a separate owner defect. x86/x64 SLEIGH
models a call as `sp -= slot; Store(sp, return_address); Call`, while the
callee's return restores the caller-visible stack pointer without a matching
caller-side `sp += slot` p-code op. Treating that scaffold subtraction and
store as persistent shifts every later stack address by one word per call.
The fix recognizes only this same-instruction three-op scaffold and excludes
its transient pointer/memory effects from the caller-visible scalar SSA.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For an ABI with ordered register parameter slots and a cspec stack pentry, a
load is an incoming stack parameter only when scalar memory SSA proves both:

1. its address is exact in the entry-SP-relative stack coordinate; and
2. every reaching memory value is function-entry input (directly or through
   an all-entry-input phi), never a store performed by the current function.

When that proof succeeds, an address at or above the first entry-stack
argument and aligned to the ABI slot size has formal parameter index:

    register_parameter_count + stack_slot_index

The proof is independent of a guessed frame size or lexical base register.
Addresses below the first incoming slot, misaligned addresses, stores,
unknown/bounded pointer guards, and mixed input/store phis remain ordinary
stack storage. A call return-address scaffold has zero net caller-visible
stack effect and is removed only from this analysis model, not from raw p-code.
```

- [x] The production rule uses only cspec, ABI-slot, exact pointer, and
      reaching-memory ownership facts.
- [x] It contains no function, address, project, compiler, or corpus guard.
- [x] Evidence spans five source functions, four projects, and stripped O0
      plus O2 binaries.
- [x] The same rule covers SysV x64 and Win64; their register counts and stack
      bases remain cspec data rather than ISA-local policy copies.

## 4. Risk And Ownership Check

- Existing owner: `fission-pcode::midend::abi::AbiState`, consumed by the
  canonical stack-slot builder.
- No new pass, fact map, production dependency, or vendor dependency.
- Win64 shadow/home space is below the cspec's first stack parameter and must
  remain `HomeSlot`, not become parameters 5-8.
- Outgoing call staging, ordinary locals, and overwritten incoming slots have
  current-function memory definitions and must remain non-parameters.
- Unknown or bounded pointer guards, non-entry-owned phis, and misaligned
  addresses are rejected rather than guessed.
- Call scaffold recognition requires a stack-pointer-sized subtraction,
  same-address return-address store, and immediate `Call`/`CallInd` at the
  same machine-instruction address.
- The implementation recovers only parameters actually accessed by the
  callee. Unused parameters such as `stophandler(int sig)` still require
  caller/callback prototype evidence and remain out of scope.
- Stack-slot access width supplies the initial parameter lattice. Named
  aggregate and pointer recovery remains a subsequent type-constraint task.

## 5. Validation Matrix

- [x] ABI tests for SysV x64 and Win64 stack slot numbering after register
      slots, shadow-space containment, and misalignment.
- [x] Builder tests: an entry-owned seventh SysV x64 stack load becomes
      `param_7`; the same slot after a current-function store does not.
- [x] Scalar-SSA test: a SLEIGH call return-address scaffold does not shift a
      later stack load or become a persistent memory definition.
- [x] Focused scored reruns for all five sample rows, NIR and HIR. Exact used
      stack parameters recovered in `socket_open2` (10), `copy_reg` (9), and
      `ssh_agent_sign` (7; source slot 8 is unused). The two loop-carried cases
      remain conservatively unpromoted because entry ownership is not proven.
- [x] Focused stripped O0 reruns for the three non-scored variants: 7, 9, and
      7 accessed parameters respectively.
- [x] Official type scorer replay on the fixed 109-function subset: perfect
      6 -> 6, total distance 1,206 -> 1,205, one improved row, zero regressed
      rows, pointer-miss rows 76 -> 76.
- [x] Full sample NIR/HIR sweeps: 250/250 functions and 224/224 binaries in
      both layers, no failures; NIR gotos 1,079 -> 1,079 and HIR gotos
      1,077 -> 1,077. Exactly four output files changed in each layer, all
      explained by accessed incoming stack values (`socket_open2`, `newtoold`,
      `copy_reg`, and `ssh_agent_sign`).
- [x] `cargo nextest run -p fission-pcode`: 1,015 passed, one skipped.
- [x] `cargo check -p fission-pcode -p fission-decompiler`
- [x] `cargo build --release -p fission-cli`
- [x] External Docker cache-disabled regression run. The current runner
      expanded `--limit 40` without `--variant-limit` to 352 rows; the exact
      40 keys from the saved fixed-denominator baseline all matched and had
      byte-identical NIR plus identical metric/status fields: semantic perfect
      22/40 and mean 0.6442; type perfect 22/40 and mean 0.8127; GED perfect
      15/40 and mean 5.225; 22 ok / 9 compile errors / 9 assertion failures.
      This is local regression evidence only, not publishable leaderboard data.
- [x] `scripts/check/owner_boundaries.sh` and
      `python3 scripts/audit/nir_boundary_scan.py --root .`: zero findings.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Row identities occur only in this local proposal. Production code and
  synthetic tests use ABI/frame invariants only.

## 7. Review Notes

- [x] The justification is metric-independent: an accessed incoming ABI stack
      slot is part of the function's formal parameter interface, not a local.
- [x] The implementation extends the existing ABI classifier and stack-slot
      binding owner.
- [x] The sample-set motivates and measures the defect; stripped O0 variants
      provide the mixed-optimization generality check.
