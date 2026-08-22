# Preserve API-proven pointer types across call aliases

## 1. Baseline Row Anchor

Measured on current `main` (`c05e2725e`) against the 250-function DecBench
sample set in `vendor/decbench-evalkit/decbench-evalkit-sample-set`.  The
official `v0.2.1` result supplies historical type distance; current local NIR
is the implementation baseline.

| binary | source function | address | source parameter | current NIR | v0.2.1 type distance |
| --- | --- | ---: | --- | --- | ---: |
| `bin_187.elf` | `wcomment` | `0x18a5` | `FILE *fp` | `ulonglong param_1` | 1 |
| `bin_050.elf` | `print_rta_ifidx` | `0x1a7de` | `FILE *fp`, `const char *prefix` | `ulonglong param_1`, `ulonglong param_3` | 3 |
| `bin_109.elf` | `get_string` | `0xd3d7` | `FILE *file`, `const char *terms` | `ulonglong param_3`, `ulonglong param_4` | 3 |

All three outputs already contain API-proven pointer aliases.  For example,
`wcomment` has `FILE* xVar8`, assigns it from `param_1`, and passes the value to
`fputs`/`fprintf`, while the source parameter remains an integer.  In
`get_string`, `char* __s = param_4` is passed to `strchr`, but `param_4` remains
an integer.

This shape also repeats in the unscored mixed-optimization `corpus/dev` split
(`open_reader`, `read_line`, `parse_prefix`, and `file_size` across multiple
programs and compiler/optimization tuples).  Those rows are regression and
generality evidence, not the optimization target.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize/type recovery
- [ ] Structuring
- [ ] Printer
- [ ] Benchmark/automation

`types/callsite_type_prop.rs::apply_callsite_type_prop_pass` resolves known API
parameter declarations and sets the immediate argument binding's
`surface_type_name`, but does not carry that declaration to a stable copy
source. The main normalize pipeline runs before structured run-scoped copy
propagation, so a call may still use an alias when the type pass runs and use
the source parameter only after the last type opportunity has passed.

The first missing fact is therefore in call-site surface propagation and its
pipeline sequencing, not in rendering. Replacing integer `NirType` globally
was tested and rejected: reused call-result/register names gained pointer types
and hundreds of compensating casts. Re-running the whole type fixed point after
layout was also rejected because it reconsidered unrelated signedness,
aggregate, return, and pointer-depth facts in 122 NIR files.

## 3. Generality / Invariant Proof

Generalized rule:

```text
An exact, informative API prototype is semantic evidence for the value passed
at that call site. Its C surface declaration may travel backward through a
plain COPY only while every traversed name is single-definition and
non-self-referential. Existing surface declarations remain locked. A generic
void-pointer contract applies to the immediate call argument only, because it
does not prove the copy source's pointee type. Reused register/temp names and
internal NirType facts are not globally rewritten.
```

- [x] No ISA, function, address, binary, or corpus identity is part of the rule.
- [x] Pointer width comes from the function/data model rather than an x86 register name.
- [x] The synthetic test states only type and COPY/dataflow facts.

Comparable coverage:

- Three sample-set functions in three different source projects repeat the
  defect, including both `FILE*` and `char*`.
- Held-out `corpus/dev` repeats the owner shape across O0/O2 and 32/64-bit
  variants; only four O0 rows expose a newly recoverable stable source.
- Synthetic coverage proves stable-copy propagation, locked-surface retention,
  generic-`void*` containment, and reused call-receiver containment.

## 4. Risk And Ownership Check

- Existing owner: `types/callsite_type_prop.rs`; downstream COPY equality remains
  owned by `types/type_flow.rs`.
- Shared fact: exact API signature plus existing type constraint graph.
- No new pass, metric, dependency, or vendor runtime use is required.  The
  existing call-site pass is sequenced once more after structured run-scoped
  copy propagation, only when that propagation changed the body.
- Principal risk: carrying a call-use type through a reused PreHIR name or
  erasing a more specific source pointee with `void*`. The shared TypeFlow
  single-definition/non-self-reference proof and the generic-void barrier are
  both mandatory.
- Known no-change cases: unknown/internal callees, uninformative database `int`
  placeholders, differently-sized integers, constants, and existing locked
  conflicting surface declarations.  Late return, signedness, aggregate, and
  general pointer-depth inference are not rerun.  A generic `void*` call
  parameter remains local to the immediate argument because it does not prove
  the original copy source's pointee type.
- Telemetry: existing call-signature refinement counters only.

## 5. Validation Matrix

- [x] Six targeted tests: immediate surface recovery, stable-copy source,
  locked surface, generic `void*`, reused call receiver, and multi-definition
  copy-source containment.
- [x] `cargo nextest run -p fission-midend-normalize -p fission-pcode`
  (1340 passed, 1 skipped)
- [x] `cargo check -p fission-pcode`
- [x] `cargo build --release -p fission-cli`
- [x] Focused NIR/HIR rows:
  - `wcomment`: `ulonglong` -> `FILE*`
  - `print_rta_ifidx`: first parameter `ulonglong` -> `FILE*`
  - `get_string`: fourth parameter `ulonglong` -> `char*`
- [x] Full sample set, fixed 250/250 functions and 224/224 binaries:

| layer | changed files | changed source signatures | gotos | `if` | `switch` | `&&` / `||` | pointer casts |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| NIR | 50 | 9 | 1079 -> 1079 | 2462 -> 2462 | 1 -> 1 | 126/94 -> 126/94 | 37 -> 37 |
| HIR | 99 | 9 semantic (17 first-line text changes including names) | 1077 -> 1077 | 2447 -> 2447 | 1 -> 1 | 124/91 -> 124/91 | 34 -> 34 |

The nine NIR signature changes were checked against the sample source and all
move toward its declaration: four `FILE*`, four `char*`, and one `__uid_t`.
The first attempted generic `void*`
propagation changed `env_free(char **envp)` to `void*`; the final barrier
restores that row to no change.

- [x] Held-out direct comparison against `c05e2725e`: 16 combinations over
  `file_size`, `open_reader`, `read_line`, and `parse_prefix`; four O0
  signatures improved to their source `FILE*`/`char*` declarations and the
  other 12 were unchanged.
- [x] Workspace-wide `cargo fmt --all -- --check` was attempted and fails on
  extensive pre-existing unrelated formatting debt; no broad formatting was
  applied.
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .`: 0 findings.
- [x] Pass-gate function/address pattern scan: no matches.
- [x] Docker external runner remains unavailable because the local OrbStack
  socket cannot connect. These are local sample measurements, not an official
  leaderboard claim.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Row identities are used only for local measurement.  Production conditions
  contain semantic prototype/type-width facts only.

## 7. Review Notes

- [x] The justification does not mention the metric: a value proven to be a
  pointer by an exact call contract must be declared as that pointer at its
  stable source rather than only at a disposable call alias.
- [x] Production code will contain no hardcoded sample identity.
- [x] The existing type owner is extended; no duplicate pass is introduced.
