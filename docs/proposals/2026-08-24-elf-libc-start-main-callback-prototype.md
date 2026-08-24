# Recover the ELF program callback prototype from `__libc_start_main`

## 1. Baseline Row Anchor

Measured on `main` at `614498173b` with the official DecBench sample-set type
scorer and the fixed 109-function DWARF-bearing subset. The current result is
6 type-perfect functions and total type distance 1,199.

Eleven scored Linux `main` functions across coreutils, base-passwd, shadow,
gnutls, findutils, libacl, iproute2, and sysvinit lose or mistype `argc` and
`argv`. Ten are O2-noinline and one is O0. Representative measured rows:

| row | optimization | current signature | type distance |
| --- | --- | --- | ---: |
| coreutils `touch` | O2-noinline | `ulonglong main(uint *, ulonglong *)` | 11 |
| gnutls `gnutls-cli` | O2-noinline | `uint main(void)` | 36 |
| iproute2 `rtmon` | O0 | `uint main(void)` | 4 |

The three binaries are stripped PIE ELF64 executables. Their entry points are
different, but raw p-code proves the same fact in all three:

```text
Copy first_cspec_integer_parameter <- constant(scored_function_address)
Copy call_target <- RAM[relocation_named___libc_start_main]
CallInd call_target
```

The callback addresses are respectively the exact scored function entries.
The relevant GOT relocations are loader facts named
`__libc_start_main@GLIBC_2.34`. This is type/signature recovery work; no
structure or recompilation improvement is claimed before remeasurement.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery and static decompilation facts
- [ ] Printer
- [ ] Benchmark/automation

Raw p-code already preserves both the callback constant and indirect-call
target storage. `ProgramSnapshot` already owns the entry point, functions,
imports, and relocations. `FactStore` is the existing analysis overlay that
feeds `build_nir_function_hints`; the wrong fact is the absence of an ABI
prototype hint for the proven callback, not a lift or printer defect.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For an ELF program entry routine, if a call target resolves through loader
import/relocation facts to __libc_start_main and the reaching definition of
the call's first cspec integer-parameter slot is a constant executable
function entry, that function is the runtime program callback. When that
callback's own p-code independently proves that it consumes at least the first
two ABI input slots, the documented libc contract types them as `int` and
`char **` and types the callback result as `int`.

Reject unresolved/ambiguous imports, non-constant callback values, values not
defined in the call's own basic block, constants that are not exact internal
function entries, callbacks that do not consume both slots, and binaries
without an ELF entry point. Do not infer an exact maximum arity: a third
`envp` parameter is valid and cannot be distinguished from a two-parameter
source declaration using this contract. The startup call alone does not
distinguish `main(void)` from unused `argc`/`argv`, so it must never fabricate
parameters without the callback-side consumption proof.
```

- [x] The production rule uses ELF loader identity, import relocation,
      cspec ABI-slot, same-block reaching definition, and exact function-entry
      facts; it contains no function name, address, project, compiler, or
      corpus guard.
- [x] Register identity and ordering come from cspec, not an x86 register
      spelling or instruction mnemonic.
- [x] The defect repeats across eleven functions, eight projects, and O0 plus
      O2-noinline.
- [x] Synthetic coverage states only the p-code/import/ABI-slot invariant.

## 4. Risk And Ownership Check

- Existing owner: `fission-static::analysis::decomp::facts::FactStore`, with
  the immutable `ProgramSnapshot` and canonical `NirFunctionHints` transport.
- Shared substrates: p-code reaching definitions, cspec parameter slots,
  loader import/relocation facts, and exact function inventory.
- No new pass, runtime dependency, vendor dependency, or benchmark-side rule.
- The helper observes only the binary entry routine and only a documented
  runtime import. Ordinary calls to a function named `main`, direct symbol
  names, FID names, and entry-point naming do not participate.
- Debug information keeps priority when present. The runtime hint only fills
  missing fields through the existing hint merge.
- The rule supplies two parameter types only after callback-side arity proof,
  plus the documented return type. It must not delete or fabricate a third
  parameter.
- Non-ELF binaries, shared libraries, custom startup runtimes, unresolved GOT
  targets, and non-constant callbacks must remain unchanged.

## 5. Validation Matrix

- [x] Pure invariant tests in `fission-static` for accepted and rejected
      p-code shapes.
- [x] `cargo nextest run -p fission-static`: 79 passed, one skipped.
- [x] `cargo check -p fission-static -p fission-decompiler`
- [x] Release CLI build and focused NIR/HIR decompilation of all eleven scored
      rows; the three anchors must render `int` and `char **` for the first two
      parameters without losing existing parameters. Ten rows changed; the
      callback that does not consume both slots correctly retained its
      `void` parameter list.
- [x] Official sample-set type scorer on the fixed 109-function denominator;
      require lower total distance and zero regressed rows.
- [x] Full sample-set NIR and HIR sweeps; require 250/250 functions, identical
      goto totals, and only explained signature/type changes.
- [x] External Docker cache-disabled fixed-denominator regression run.
- [x] Held-out stripped `corpus/scale` ELF main functions from at least two
      projects and both optimization families.
- [x] `scripts/check/owner_boundaries.sh` and
      `python3 scripts/audit/nir_boundary_scan.py --root .`.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Row identities occur only in this local proposal. Production code and tests
  use the runtime callback invariant only.

## 7. Review Notes

- [x] Metric-independent justification: libc invokes the proven callback with
      `argc` and `argv`; rendering those values as absent, integers, or a
      single pointer misstates the function's externally defined interface.
- [x] The change extends the existing static-fact/type-hint owner.
- [x] The sample set motivates and measures the defect; held-out stripped
      binaries provide the independent regression/generalization gate.

## 8. Measured Result

- Fixed official sample subset: 109/109 type-measured functions. Type-perfect
  remains 6, while total type distance falls from 1,199 to 1,187 and rows with
  pointer misses fall from 76 to 67. Ten rows improve and none regress.
- The ten admitted rows render an `int` result followed by `int argc` and
  `char ** argv`. Existing third parameters remain present in the two
  three-parameter callbacks. The gnutls callback that does not read both
  incoming slots remains `uint main(void)`.
- Full sample-set output remains 250/250 functions across 224/224 binaries in
  both layers. NIR remains at 1,079 gotos and HIR at 1,077; the only changes
  attributable to this slice are the ten measured callback signatures.
- Three stripped, non-scored `corpus/scale` variants across coreutils,
  iproute2, and base-passwd reproduce the interface recovery under O0 and
  O2-noinline.
- `cargo nextest run -p fission-decompiler -p fission-pcode` passes 1,076
  tests with one skipped. Owner-boundary, NIR-boundary, and benchmark-smell
  scans report zero findings.
- The final local Linux bundle passed the cache-disabled external Docker
  `corpus/dev` fixed 40-row gate with 40/40 direct-function coverage and zero
  metric-cache hits. Its decompiled NIR/HIR, statuses, semantic/type/GED
  scores, goto counts, and readability fields are byte-identical to the
  pre-change baseline: semantic 22/40 perfect (mean 0.6442), type 22/40
  perfect (mean 0.8127), and GED 15/40 perfect (mean distance 5.225).
