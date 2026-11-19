# A test of a register copied from a global tests the global

Date: 2026-11-20. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Found while measuring #933, where these functions left `equivalent`:

| function | config | machine | printed |
|---|---|---|---|
| bash `rl_set_prompt` (`0xd01f0`) | `-O2` | `mov rdi, [rl_prompt]; test rdi, rdi; je` | `if (param_1) free(rl_prompt);` |
| bash `dispose_exec_redirects` (`0x4a220`) | `-O2` | `mov rdi, [exec_redirection_undo_list]; test rdi, rdi; je` | `if (param_1) ...` |

Why the output is wrong, without a metric: the branch tests the global, and
the printed code tests the function's first argument, which the machine
never reads.

## 2. Owner Proof

- [x] Builder, `control/terminator.rs` (`lower_flag_tested_value`): for a
  tested register whose definition is a copy with no binding of its own, the
  predicate used the register's hardware name (`rdi`) -- on the expectation
  that the copy is emitted under that name, which holds for a source the
  code may overwrite before the test. A copy of a global is not emitted
  under it, so nothing assigned `rdi`, and entry-parameter promotion named
  the unassigned `rdi` as `param_1`.

## 3. Generality / Invariant Proof

- [x] Not ISA-specific in its rule: "a copy whose source reads no register".

Invariant: a copy of a global or a constant reads no register, so its value
is the same at the copy as at the test; it is read at the copy. A store to
the global between the two already gives the copy a binding of its own
(#931), which is checked first. Other sources keep the register's name:
`mov rdi, [rdi + 0x30]` writes its own base, and re-reading the load at the
test would follow the new `rdi`.

## 4. Risk And Ownership Check

- A copy from a computed (unique) source still falls back to the register's
  name. Where that copy is never emitted under the name, the predicate
  still reads an unassigned register (`if (param_1)` for
  `mov rdi, [rdi + 0x30]; test rdi, rdi` when the load is not materialized).
  Deciding that needs to know whether the copy will be emitted, which the
  terminator is lowered before; left open.

## 5. Validation Matrix

- [x] Targeted test: `a_test_of_a_register_loaded_from_a_global_tests_the_global`
  (fails with the fix reverted, printing `if (param_1)`); its second half --
  a store to the global between the copy and the test -- checks the snapshot
  path still owns that case.
- [x] Crate gate: `cargo nextest run -p fission-pcode -p fission-decompiler
  -p fission-midend-normalize`; CI clippy line clean.

### Measured

`fission_cli verify --all --tier ground-truth`, 16 sample binaries, 15,280
functions. Measured alone against `main` (`b62197539`), and together with
#933, which gives these globals their file scope:

| verdict | main | + this | #933 | #933 + this |
|---|---|---|---|---|
| equivalent | 5,571 | 5,592 | 5,965 | 6,004 |
| diverged | 948 | 921 | 959 | 922 |
| unsupported | 8,439 | 8,445 | 7,998 | 7,996 |
| harness | 322 | 322 | 358 | 358 |

With #933: diverged → equivalent 34, unsupported → equivalent 5, nothing
leaves equivalent. Alone: 4 functions go equivalent → unsupported
(`dispose_exec_redirects`, `dispose_partial_redirects`, `check_read_timeout`,
`rl_set_prompt`) -- the predicate now reads the global, which on `main`
alone is still declared as an uninitialised local (the defect #933 fixes),
so the verifier stops on the uninitialised read.

A first version read any non-register source at the copy. It turned two
functions diverged (an sshd `mov rdi, [rdi + 0x30]; test rdi, rdi`
re-read the field through the overwritten `rdi`) and moved two DecBench
rows; restricting it to globals and constants removed both.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`): perfect 67/233,
total distance 7,482 before and after, every row identical. Not a quality
claim.

## 6. AI Review / Prompt Firewall

No external model was consulted.
