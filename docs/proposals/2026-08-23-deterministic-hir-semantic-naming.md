# Make HIR semantic naming deterministic

## 1. Baseline Row Anchor

Measured on `main` at `7f0be19af` by decompiling each function five times
with the release CLI and hashing `code_nir` and `code_hir` separately.

| sample | project / optimization | NIR hashes | HIR hashes |
| --- | --- | ---: | ---: |
| `bin_003.exe` | x0r-usb / O2 | 1/5 unique | 5/5 unique |
| `bin_039.elf` | coreutils / O2-noinline | 1/5 | 5/5 |
| `bin_064.elf` | rsyslog / O0 | 1/5 | 5/5 |
| `bin_093.elf` | gnutls / O2-noinline | 1/5 | 5/5 |
| `bin_128.elf` | sysvinit / O2-noinline | 1/5 | 3/5 |
| `bin_179.elf` | openssh-portable / O2-noinline | 1/5 | 5/5 |

The differing HIR texts use different assignments of generated names such as
`ptr`, `p`, `addr`, and `ptrN`; the NIR text is byte-identical. This is a
determinism defect, not a semantic or structure-distance improvement claim.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [x] HIR presentation
- [ ] Printer
- [ ] Benchmark/automation

`render/presentation/naming/pointer_naming.rs` collects candidates in a
`std::collections::HashMap`, converts it to a vector, and sorts only by score.
Equal-score candidates retain the map's randomized iteration order before
receiving `ptr`/`p`/`addr` names. `naming/mod.rs` repeats the same partial sort
for candidates from all detectors; size candidates all carry the same score.

```text
same NIR tree
  -> randomized HashMap iteration among equal-score naming candidates
  -> different HIR-only alpha-renames on every process invocation
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Every HIR presentation candidate order must be a total order. Rank by the
existing semantic confidence score first and use the original binding name as
the deterministic tie-break. The same HirFunction must therefore produce the
same rename mapping independently of process hash seeds.
```

- [x] No ISA, ABI, compiler, function, address, or corpus identity is used.
- [x] The rule changes only tie ordering between candidates already admitted
      at exactly equal confidence.
- [x] The synthetic test uses two equal-score generic pointer bindings and
      asserts the canonical mapping.

Comparable coverage exceeds the row gate: six functions, six projects, and
O0/O2/O2-noinline inputs all reproduce the same owner defect.

## 4. Risk And Ownership Check

- Existing owner: `apply_semantic_naming` and its pointer detector.
- Shared substrate: none; this is owner-local ordering.
- No new pass, helper, dependency, telemetry, or semantic fact is required.
- Known cases that must not change: NIR output, candidate admission scores,
  effects/evaluation order, types, control flow, and already meaningful names.

## 5. Validation Matrix

- [x] Equal-score pointer naming invariant test.
- [x] Focused five-run hashes for all six measured sample rows: one NIR and one
      HIR hash per row.
- [x] `cargo nextest run -p fission-pcode -- naming` (6/6 passed).
- [x] `cargo nextest run -p fission-pcode` (1011 passed, 1 skipped).
- [x] Full sample NIR/HIR coverage and goto comparison: 224/224 binaries and
      250/250 functions; NIR remained at 1,079 gotos and HIR remained at 1,077.
- [x] `scripts/check/owner_boundaries.sh`
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .`

Measured result: all six rows changed from 3-5 distinct HIR hashes in five
runs to one, while every row retained one NIR hash. This establishes
reproducible HIR alpha-renaming without claiming a structure, type, or
recompilation-score change.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Production code will contain no benchmark identity.
- The patch-validation evidence is the synthetic total-order invariant plus
  the six multi-project, mixed-optimization real rows.

## 7. Review Notes

- [x] Production code will contain no hardcoded row guards.
- [x] The justification is metric-independent: identical input producing a
      different public HIR text is a reproducibility defect.
- [x] The change extends the existing naming owner and introduces no duplicate
      presentation pass.
