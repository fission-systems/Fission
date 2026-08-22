# Reclaim canonical names after stack-slot collision owners disappear

## 1. Baseline Row Anchor

Measured on current `main` (`4ea85dff1`) with the official DecBench
`type_match.py` implementation and the sample-set submission relabeling path.
At validation time, the expected `corpus/scale` path contained DWARF-bearing
originals for a fixed 109-function subset; that subset had 5 type-perfect rows
before this change.

| binary | function | address | current declaration | ground-truth slot |
| --- | --- | --- | --- | --- |
| `bin_064.elf` | `parseRFCStructuredData` | `0x29c5f` | `uchar * local_8_7` | `char *p2parse`, `-8` |
| `bin_199.elf` | `zlibCompileFlags` | `0x1068b` | `longlong local_8_1` | `uLong flags`, `-8` |

`parseRFCStructuredData` is one type edit from perfect: 6/7 variables match,
and the only missing variable is the live `local_8_7`.  The current output
uses that binding throughout the function, but its stale collision suffix
prevents the stack coordinate from being represented by the canonical
`local_8` name.  The row has no perfect GED, type, or byte-match score in the
published sample result, so recovering it is also a potential Union gain.

The same census found 18 orphaned `local_<offset>_<id>` names in 15 sample
files.  They span O0, O2, and O2-noinline output and projects including
rsyslog, zlib, coreutils, cronie, gnutls, openssh, and iproute2.  In every
counted case the unsuffixed base name is absent from the final function.

The focused commands are:

```text
fission_cli decomp .../bin_064.elf --addr 0x29c5f --layer both --prehir --json
fission_cli decomp .../bin_199.elf --addr 0x1068b --layer both --prehir --json
```

This is alpha-renaming/storage-identity work.  No behavior-case improvement is
claimed.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [x] Normalize cleanup
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

`PreviewBuilder::ensure_stack_slot_binding` creates the canonical name from
the resolved stack coordinate and calls `unique_stack_slot_binding_name` only
when that name is already occupied.  The suffix therefore records a real
collision at creation time.  `used_param_local_names` is deliberately
over-inclusive and never reclaims names after later normalization removes the
old owner.

```text
builder: local_8 already occupied -> surviving slot becomes local_8_7
cleanup: old local_8 binding disappears
final:   local_8_7 remains although local_8 is now free
```

The wrong final fact is not created by the printer or scorer: the final NIR and
HIR trees already carry `local_8_7`.  Normalize cleanup is the first owner that
knows the colliding binding is gone and can safely perform an alpha rename.

## 3. Generality / Invariant Proof

Generalized rule:

```text
After final dead-binding cleanup, a surviving stack-derived binding whose name
is a collision suffix may reclaim its canonical stack-coordinate base name iff
that base is no longer occupied and exactly one survivor claims it.  Rename the
binding and all of its uses atomically.  Do not merge bindings or alter types,
storage origins, expressions, control flow, or evaluation order.
```

ISA-agnostic check:

- [x] The condition uses binding origin and stack-slot identity, not an ISA,
      compiler, function, address, or corpus identifier.
- [x] No register spelling or calling-convention special case is added.
- [x] The synthetic test states only the post-prune binding/name invariant.

Comparable coverage:

- `parseRFCStructuredData` (rsyslog O0), `zlibCompileFlags` (zlib O0), and
  `cmp`/`copy_reg`-family coreutils rows repeat the suffix-after-prune shape.
- O2 and O2-noinline examples occur in cronie, coreutils, gnutls, openssh,
  iproute2, libselinux, and shadow, so the shape is not O0-only.
- Synthetic coverage will include a single orphan that is renamed, an occupied
  base that is preserved, and two survivors for one base that both stay
  distinct.

## 4. Risk And Ownership Check

- Existing owner: final normalize cleanup in `pipeline/stages.rs`, immediately
  after dead bindings and undeclared-binding rescue have stabilized.
- Shared substrate: the existing `rename_vars_in_stmts` PreHIR helper.
- A small owner-local helper is needed because pruning and alpha-renaming are
  separate operations; changing builder name allocation would reuse names
  before it knows whether the original owner survives.
- The helper must require a stack-derived origin, a syntactically valid
  collision suffix, a free base, and a unique claimant.  Parameters, temps,
  home slots, live collisions, and ambiguous multiple survivors must not
  change.
- New owner-to-owner dependency: none; normalize already depends on the PreHIR
  rename helper.
- Telemetry: none.

## 5. Validation Matrix

- [x] Targeted invariant tests for orphan, occupied-base, and ambiguous-base
  cases.
- [x] `cargo nextest run -p fission-pcode`
- [x] `cargo check -p fission-pcode`
- [x] `cargo build --release -p fission-cli`
- [x] Focused NIR/HIR reruns for `bin_064` and `bin_199`.
- [x] Official type scorer replay on the fixed 109-function sample subset.
  `parseRFCStructuredData` became type-perfect with no perfect-row regression.
- [x] Full 250-function sample-set NIR and HIR sweeps with fixed coverage and
  per-file diffs.
- [x] Docker Linux GCC recompilation for changed outputs where the source
  toolchain is available.
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .`

Measured results:

- Targeted normalize tests: 3/3 passed; full normalize 336/336 and PreHIR 4/4
  passed. `fission-pcode`: 1,010/1,010 passed (1 skipped). `cargo check` passed
  for both `fission-pcode` and
  `fission-decompiler`; the release CLI build passed.
- Sample NIR: 224/224 binaries and 250/250 functions. Gotos stayed
  1,079 -> 1,079. Fifteen files changed; all changes were 17 unique stack-name
  reclaims covering 84 declaration/use occurrences. No expression, statement,
  type, or control-flow text changed after applying those alpha-renames.
- Sample HIR: 224/224 binaries and 250/250 functions. Gotos stayed
  1,077 -> 1,077. A byte-for-byte file comparison is not a valid regression
  signal here because the existing HIR name allocator is nondeterministic: the
  same unchanged function produced three different output hashes in three
  consecutive runs. The focused rows carry the reclaimed name in both NIR and
  HIR; the HIR nondeterminism is a separate defect.
- Official type scorer replay on the fixed subset: perfect rows 5 -> 6, total
  type distance 1,215 -> 1,214, mean accuracy 0.190363 -> 0.191673. The only
  score change was `parseRFCStructuredData`, 6/7 -> 7/7.
- Docker external runner (`corpus/dev`, 40 functions, one GCC O0 variant,
  cache resume disabled): 40/40 direct-function coverage, semantic perfect
  22/40, type perfect 22/40, GED perfect 15/40, and Linux GCC compilable
  29/40. This is a regression observation, not a sample-set ranking claim.
- Official DecBench fixup under Docker GCC 16.1 preserved compile status for
  every one of the 15 changed sample files. One pair compiled on both sides
  (`bin_199`) and its `.text` SHA-256 was identical; the other 14 failed on
  both sides at the same fixup iteration.
- NIR boundary scan: 0 findings, 0 violations, 0 migration debt.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Production logic will contain no benchmark identity.
- The patch-validation evidence is the synthetic invariant test plus the
  multi-project, mixed-optimization sample census.

## 7. Review Notes

- [x] Production code will contain no hardcoded binary/function/address/corpus
  guards.
- [x] The justification is metric-independent: a stale allocation collision
  suffix no longer describes the final binding set or the binding's canonical
  stack coordinate.
- [x] The change extends final normalize cleanup and reuses the shared rename
  substrate; it adds no semantic pass or metric.
