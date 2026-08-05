# Decompiler Change Proposal: HIR-Level Semantic Variable Naming

Date: 2026-08-06

## 1. Context

The dashboard's readability score compares Fission's HIR pseudocode against
Ghidra's. The most visible gap: Ghidra's decompiler front-end itself always
falls back to `param_N`/`iVarN`-style placeholders with no symbol info (this
was confirmed by reading `database.cc::buildVariableName` in the vendored
`vendor/ghidra` source -- Ghidra's *own* polished names people are used to
seeing come from PDB/DWARF/FID-signature matching, not a naming heuristic in
the core decompiler). Fission has no such heuristic at all: every parameter
and temp that isn't DWARF-sourced (`NirFunctionHints`, confirmed to be
entirely DWARF-derived) stays `param_N`/`uVarN`/`iVarN`/`xVarN`/`bVarN`
forever, on both NIR and HIR layers.

Read `vendor/angr-master`'s `analyses/decompiler/semantic_naming/` module as
a concrete, battle-tested design template: a priority-ordered chain of
independent usage-pattern detectors (pointer dereference/arithmetic/
linked-list traversal, well-known libc argument roles, loop induction
variables), each producing score-gated candidate renames, with
already-claimed variables excluded from later, lower-priority passes.

## 2. Design

New `render/presentation/naming/` module, wired into
`apply_hir_presentation_passes` (HIR layer only, run once after the
structural fixed-point loop settles -- loop-counter detection needs the
final `While`/`For`/`DoWhile` shape, and running once avoids each earlier
iteration's transient tree shapes producing conflicting guesses). **Not**
applied to NIR: NIR is the semantic-faithful, oracle-scored layer and must
stay mechanically derived from the pcode, per ADR 0011.

- `loop_counter_naming` (priority 10, highest): `For`/`While`/`DoWhile`
  induction variables get `i`, `j`, `k`, ... by nesting depth.
- `pointer_naming` (priority 45): a bare `Var` used as a load/store address
  scores as a dereference (25); a `PtrOffset` base scores as pointer
  arithmetic (20); `dst = *(dst + k)` (single-statement self-update, the
  classic `cur = cur->next`) scores as linked-list traversal (30) and picks
  from an iterator-flavored name pool (`cur`/`iter`/`node`) instead of the
  generic pool (`ptr`/`p`/`addr`). Threshold 20.
- `size_naming` (priority 70, lowest): a small explicit table (~19 libc
  functions -- `memcpy`, `malloc`, `fread`, ...) maps call argument
  position to a suggested name (`n`, `size`, `count`, ...), mirroring
  angr's own hardcoded `SIZE_PARAM_FUNCTIONS` table rather than a general
  signature-database lookup (no version skew to track for this fixed
  ecosystem-wide set).

Only touches bindings still matching the generic placeholder shapes
(`is_renamable_temp_name`) -- DWARF-sourced names, user renames, or an
earlier semantic-naming rename are never revisited. Renaming is a pure
relabel: `rename_var_everywhere` updates the binding and every read/write in
the body atomically, so it changes no evaluation count, order, or
observable side effect (ADR 0011-compliant by construction; verified it
does not trip the existing structural-invariant firewall in `invariants.rs`).

## 3. Verification

- `cargo nextest run -p fission-pcode --lib presentation::naming`: 4/4 new
  unit tests pass (dereference rename, loop-counter rename, `memcpy` size
  rename, generic-name-already-meaningful no-op).
- `cargo nextest run --workspace --no-fail-fast`: 2305/2312, same 7
  pre-existing unrelated `fission-emulator` baseline failures as established
  earlier this session -- no regressions.
- Real corpus, `--layer hir` (the layer this pass actually affects --
  confirmed via a first failed attempt that only checked `--layer nir`,
  which by design never runs `apply_hir_presentation` and therefore never
  shows renames):
  - `list_sum @ advanced_patterns_gcc_O2.exe`: `param_1` (a pointer
    dereferenced with `*param_1` and advanced via `param_1 = *(param_1+2)`
    across a loop) → `ptr`.
  - `find_pair_value @ data_structures_gcc_O2.exe`: same pattern, `param_1`
    → `ptr`.
  - `fibonacci @ math_gcc_O2.exe`: a `do`-loop's exit-condition variable
    (`i != rdi`) → `i`.
  - Swept 7 more corpus binaries (`crypto`, `memory_layouts`,
    `string_utils`, `advanced_patterns`, `data_structures`, `math`, gcc
    `-O2`) end to end with no panics or invariant-firewall reverts.
- Confirmed the dashboard's readability metrics (`runner/report.py` via
  `runner/runner.py`) read `code_hir`, the exact layer this pass targets --
  the oracle/scoring path (`docker/fission/server.py`) explicitly requests
  `--layer nir`, so this change has zero effect on scored correctness, only
  on the human-facing readability surface.

## 4. Known limitations, not fixed this round

- `check_linked_list_self_update`'s pattern requires the pointer advance to
  be a single self-referential statement (`dst = *(dst + k)`). Real -O2
  output often splits this across two statements via an intermediate
  temp (`t = *(dst + k); dst = t;`), which the plain dereference/arithmetic
  scoring still catches (hence `list_sum`/`find_pair_value` above both
  still get renamed), but not the linked-list-specific `cur`/`iter`/`node`
  bonus -- they land in the generic `ptr` pool instead. Not a correctness
  issue, just a slightly less specific name than optimal.
- Loop-counter naming only looks at `is_renamable_temp_name` bindings
  (`param_N`/`uVarN`/etc). Many -O2 loop counters are already-mechanical
  register names like `rax`/`rbx` (an existing, separate naming convention),
  which this pass deliberately leaves alone rather than guessing whether
  they're "generic enough" to touch.
- Only three of angr's five `NAMING_PATTERNS` passes were ported
  (`ArrayIndexNaming`, `CallResultNaming`, `BooleanNaming` were not). Left
  for a follow-up round once these three are confirmed stable in the wild.
