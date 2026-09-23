# Preserve scalar storage widths in aggregate field declarations

## 1. Baseline Row Anchor

- Binary: `memory_layouts_gcc_O2.exe` from the external dev corpus.
- Function/address: `manipulate_bitfields@0x140001530`.
- Baseline Fission source: `090004ae0`; local benchmark service reports source
  fingerprint `f88cbdab392b47cf608480a06b11174486b2eac03654f65a09a9cf87b43c5db9`.
- External command:

  ```bash
  python runner/runner.py --corpus dev --function manipulate_bitfields \
    --decompilers fission --output /tmp/fission_issue73_before_090004ae.json \
    --no-resume --run-mode local
  ```

- Baseline: 9 compiler rows, semantic mean `0.0`; all 9 stop at compile time
  because the semantic harness redeclares `ConfigNode` with a conflicting
  recovered typedef. Bare compilation passes after uniform fixup for all 9,
  which does not isolate this issue.
- The baseline has two observable shapes: direct PreHIR captured the scalar
  `((uint *)(param_1))[1] = param_2` access, while the DecBench adapter's HIR
  already spelled the store `node->val = val`. That adapter still emitted:

  ```c
  typedef unsigned char undefined;
  typedef struct fission_agg8 {
      undefined flags;
      undefined val;
  } fission_agg8;
  ```

  Thus C lays out `val` at byte 1 and the object at 2 bytes, despite the
  recovered aggregate size of 8 and the emitted `node->val = val` access at
  the source's four-byte field offset. The source declares two four-byte
  regions; disassembly confirms `movl %edx, 4(%rcx)` at `0x140001536`.
- After the change, the no-db function output uses scalar `node->val = val`
  and byte-wide `node->flags` accesses. Its aggregate is `uchar flags;`
  followed by three bytes of padding and `uint val;`. Clang syntax-only plus
  `_Static_assert(sizeof(ConfigNode) == 8)` and
  `_Static_assert(offsetof(ConfigNode, val) == 4)` passes.
- The external dev rerun used source fingerprint
  `e6040a25ca0e635043740246a5e81a880ca0c6d2904ff71602caed67575f1184` and
  completed 9 compiler rows. The `gcc -O2` target now has the corrected field
  widths and bare compile passes after fixup. The semantic score remains
  uninformative: the same source `struct ConfigNode` / recovered typedef
  collision prevents the semantic harness from compiling, so no score gain is
  claimed.
- The whole-binary `--project | clang -fsyntax-only` command also fails on
  unrelated translation-unit errors, so it is not a usable issue-specific
  score gate. Keep the external row as a measurement anchor, not a claim that
  its current headline score measures this defect.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

The defect spans two existing normalize owners that run consecutively:
`partition::collect_partitioned_memory_accesses` initially records `Load` and
`Deref` forms but not already-recovered `Index` or `FieldAccess` expressions /
lvalues; later, `ptr_arith::recover_in_lvalue` and `recover_in_expr` rerun after
aggregate recovery, but an existing `Index` only recurses into its children
and is never reconsidered against the newly refined aggregate pointee. The
captured PreHIR is `((uint *)(param_1))[1] = param_2` with a four-byte scalar
element type; the constant index represents byte offset 4 and a 32-bit store.
The facts collector sees the byte load at offset 0 but drops the Index store,
so it lacks the second offset and scalar width needed to refine the aggregate
shape. Even after that evidence is collected, the stale scalar `Index` must be
reconciled with the now-aggregate pointer or C will scale it by the aggregate
size instead of preserving its original four-byte address.

The aggregate update also needs to preserve existing field identity:
`can_upgrade_binding_to_aggregate` accepts a populated aggregate and can
replace its field vector with the inferred shape, discarding trusted names;
`update_binding` only fills a completely empty vector. A synthetic red test
models an 8-bit load at offset 0 and a 32-bit constant-index store at offset 4
and currently fails to refine the named Unknown fields. In the real row, the
rendered aggregate still contains named Unknown fields; `print_type(Unknown)`
becomes `undefined` (the project prelude aliases it to `unsigned char`), so C
lays out `val` at byte 1 rather than its recorded byte offset 4.

This is not a printer formatting workaround: normalize already owns the
per-offset access facts, aggregate shape, and pointer-arithmetic recovery.
Include constant-index and recovered field accesses in the typed-facts
inventory, convert constant indices to byte offsets using their element type,
preserve existing aggregate layout, and enrich only Unknown member types from
same-offset facts. During the existing post-aggregate pointer-arithmetic
rerun, convert a scalar constant `Index` only when its byte offset exactly
matches a known member and the access width equals the member storage width.
For direct scalar accesses through an aggregate pointer, recover offset-zero
member access only when that known member's storage width exactly equals the
load/store width; otherwise preserve the scalar access instead of widening it
to an aggregate/member expression.
Dynamic indices, partial-width views, unknown slots, aggregate-element
indexing, and ordinary scalar-array indexing must retain their existing form.
Pointer-only consumers such as stack-slot surfacing keep their candidate
surface unchanged.

## 3. Generality / Invariant Proof

```text
For a pointer to a typed aggregate with known field offsets but unknown field
types, observed scalar loads/stores at those constant byte offsets may fill
the unknown field types. A scalar constant-index access may become a field
access only when its scalar byte offset identifies one known member exactly
and its width covers that member's storage. Preserve aggregate field identity;
never reinterpret a runtime index, an aggregate element, a partial-width
access, or a scalar-array index as a fixed field.
```

- Production rule is based on byte offsets and typed memory-access evidence;
  it has no function, address, binary, compiler, or ISA guard.
- Synthetic invariant test: an aggregate with named `Unknown` fields at
  offsets 0 and 4, accessed as an 8-bit load and a 32-bit constant-index
  store, must retain those names/offsets and acquire the corresponding scalar
  storage types. The subsequent pointer-arithmetic recovery test must rewrite
  that exact scalar index to the offset-4 member while preserving dynamic
  indices, scalar-array indexing, and aggregate-element indexing.
- Direct-access invariant test: exact-width scalar loads/stores at aggregate
  offset zero become the first member access; narrower views of a wider first
  member remain width-preserving scalar dereferences.
- Preserve-case test: a pre-existing known aggregate/scalar field type is not
  overwritten by a narrower access.
- The checked-in patch-validation manifest is go/stop-only. It currently has
  source and manifest files but no canonical runner/oracle; a one-off Clang
  O2 shared-library probe produced five decompilation units, but cannot be
  classified as regression-present/absent without a before/after oracle. Do
  not use the locked external holdout as a substitute or a tuning target.

## 4. Risk And Ownership Check

- Existing owner: `apply_aggregate_fields_pass` and its
  `TypedObjectFacts::accesses` inventory.
- Shared analysis candidate: existing memory partition/type facts; no new
  global alias analysis is proposed.
- Extend the current owner by merging same-offset observed types only into
  `Unknown` fields. Preserve non-unknown type evidence and field names.
- Possible interaction: union/overlapping views and bitfield storage can have
  different access widths. The merge must follow the existing conservative
  `merge_field_ty` policy and must not turn a known aggregate field into a
  scalar just because of one partial store.
- New owner dependency: none. Telemetry: none.
- Known cases to preserve: unknown/variable offsets, homogeneous arrays,
  explicit field types, and source-surface names.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - `scalar_constant_index_on_aggregate_pointer_recovers_exact_member`
  - `direct_scalar_access_on_aggregate_pointer_preserves_member_width`
  - `preview_type_hints_rewrites_field_access_through_pointer_cast`
  - The tests cover same-offset scalar widths, field identity, exact member
    recovery, dynamic/array preservation, and pointer-cast field renaming.
- [x] Normalize crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize`
  - Result: 411 passed.
- [x] Focused external benchmark row:
  - Command: rerun the external command above with `--no-resume`.
  - Result: target `gcc -O2` declaration and scalar store are corrected;
    standalone C size/offset assertions pass. Semantic score is still 0/9
    because the semantic harness fails on its existing `ConfigNode` typedef
    collision, so it is not used as a before/after quality score.
- [x] Downstream tests/build checks:
  - `cargo check -p fission-pcode -p fission-decompiler` passes.
  - `cargo build -p fission-cli --release --bin fission_cli` passes.
  - `cargo fmt --all --check` and `git diff --check` pass.
  - `cargo nextest run -p fission-pcode --no-fail-fast`: 1114 passed,
    3 unrelated builder tests failed in this run, 1 skipped. The failures are in
    `diamond_join_lowers_copy_through_join_read_as_select`,
    `movzx_after_byte_add_zero_extends_unsigned`, and
    `x64_byte_add_movzx_does_not_double_add_load`; none exercises the changed
    aggregate access or field-rename code.
- [ ] Patch validation pool:
  - Result: manifest/source exist, but there is no checked-in runner/oracle;
    the available manual probe is not a valid go/stop comparison. No external
    holdout rows were used.

## 6. AI Review / Prompt Firewall

- Was another AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Ghidra guidance: correctness/reference only; no output-style request.
- The concrete row is an anchor; implementation decisions are based on the
  generic offset/type-evidence invariant.

## 7. Review Notes

- Production code contains no binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-
  only edits:
  - [x] Confirmed; the semantic metric remains harness-blocked.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; existing normalize passes and shared rename utility were
    extended, with no new pass or metric.
