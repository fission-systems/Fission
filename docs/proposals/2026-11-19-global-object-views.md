# A part of a named global is accessed through its address

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

bash `-O2` `int pgrp_pipe[2]` at `0x13b738`, which the loader knows as one
8-byte object named `pgrp_pipe`:

| function | printed | machine |
|---|---|---|
| `restore_pgrp_pipe` (`0x67520`) | `pgrp_pipe = *ptr; ... tmp_13b73c = rax;` | `mov [0x13b738], eax` (4 bytes); `mov [0x13b73c], eax` |
| `save_pgrp_pipe` (`0x674f0`) | `*ptr = pgrp_pipe; rax = tmp_13b73c;` | 4-byte loads of each half |

Why the output is wrong, without a metric: in C a name is the whole object.
`pgrp_pipe = *ptr` writes 8 bytes, clobbering the second `int`, and reading
`pgrp_pipe` yields 8 bytes where the machine compares 4. `tmp_13b73c` is a
separate variable, so code that writes the object through its address
(`pipe(&pgrp_pipe)`) does not change it in the printed program.

Across the 16 sample binaries, an interior address of a named object of known
size was printed as its own `tmp_<address>` variable in 160 function/object
pairs (bash 152, iproute2 8).

## 2. Owner Proof

- [x] Builder, `lower_varnode_inner` / `global_memory_read`: a read of global
  memory with no reaching definition was the object's name when its address
  was the object's base (whatever the width), and a fresh `tmp_<address>`
  variable otherwise.
- [x] Builder, `maybe_materialize_output_stmt`: a write of global memory was
  `*(T *)&name` for the exact base only, else `*(T *)&tmp_<address>`.
- [x] Render, `globals.rs` (`recover_global_symbol_accesses`): `*(T *)&g`,
  load or store, became `g` whatever `T`'s width.
- [x] Printer, `peel_simple_deref_target`: `*&g` was spelled `*g`.

Giving those accesses the object's address exposed four more owners that
mistreated a global's address, width or scope:

- [x] Normalize, `memory/slots.rs` (`resolve_slot_alias_base`): a slot alias
  rooted at `&g + 32` resolved `&g` as the variable `g`.
- [x] Normalize, `types/constraint.rs` (`update_ast_expr`): a load took an
  unknown pointee for its type, printing `*(undefined *)` -- a read of no
  width.
- [x] Render, `layered.rs`: a global named only in a local's initializer was
  not declared.
- [x] Normalize, `cleanup/temp_var.rs` (`rescue_undeclared_bindings`): any
  undeclared name starting with `r` or `e` is taken for a register, so
  globals such as `rl_readline_state`, `rl_point` and `expanding_redir` were
  declared as uninitialised locals, cutting every read and write of them off
  from the object.

## 3. Generality / Invariant Proof

- [x] Uses the loader's symbol sizes (`global_sizes`) and the existing global
  memory test (`output_is_mapped_global_memory`); nothing ISA-specific.

Invariant: a named object is accessed by its name only as a whole. An access
of a different width, or at an interior offset, goes through the object's
address with the access's own type. Types the program never declared
(array vs. struct) are not guessed: `*(uint *)((uint8_t *)&g + 4)` is
correct for either.

16-byte objects are left to `opaque_integer_scalar_view`, which already
projects their lanes from the value (`(int)(g >> 32)`).

## 4. Risk And Ownership Check

- More casts in the text where an object is used piecewise. That is the
  access the machine makes; the old spelling was a different access.
- Reads become loads through an address rather than a bare name; normalize
  treats them as memory reads, which keeps them from moving across stores.

## 5. Validation Matrix

- [x] Targeted tests, each failing with only its fix reverted:
  `a_part_of_a_named_global_is_accessed_through_its_address` (builder view,
  `globals.rs` width check and printer; in-test control: no known size),
  `a_slot_alias_of_a_global_field_is_rooted_at_the_globals_address`,
  `a_load_keeps_its_width_when_the_pointee_is_unknown`,
  `a_global_named_only_in_a_local_initializer_is_declared`,
  `rescue_undeclared_bindings_leaves_globals_to_file_scope` (in-test control:
  an unknown name is still rescued).
- [x] Crate gate: `cargo nextest run -p fission-pcode -p fission-decompiler
  -p fission-midend-normalize`; CI clippy line clean.

### Measured

`fission_cli verify --all --tier ground-truth`, 16 sample binaries, 15,280
functions, current `main` (`b62197539`) vs. this branch:

| verdict | main | this branch |
|---|---|---|
| equivalent | 5,571 | 5,965 |
| diverged | 948 | 959 |
| unsupported | 8,439 | 7,998 |
| harness | 322 | 358 |

Transitions: unsupported → equivalent 342 (most were reads of a global the
old output had declared as an uninitialised local), diverged → equivalent
63, unsupported → diverged 65, unsupported → harness 36.

Out of equivalent, 11 (10 diverged, 1 unsupported). The ones read all
change only where a global is declared -- file scope instead of an
uninitialised local -- and diverge now that the global holds its real
value, exposing defects present on `main`:

- `dispose_exec_redirects`, `rl_set_prompt`: `mov rdi, [g]; test rdi, rdi`
  prints `if (param_1)`, the register's entry value, on both sides. `main`
  read the uninitialised local and was checked on one argument set only.
- bash `-O0` `0xfe845`: `or ah, 2` on a loaded global is stored as
  `g = (uchar)(g / 256) | 2`, the open AH read-modify-write defect from #846.

`tmp_<address>` names for the interior of a named object of known size:
160 function/object pairs on `main`, 0 here.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`): perfect 67/233,
total distance 7,482 before and after, every row identical. Not a quality
claim.

## 6. AI Review / Prompt Firewall

No external model was consulted.
