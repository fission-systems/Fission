# Recover declarations for undeclared SIMD lane temporaries

## 1. Baseline Row Anchor

- Binaries/functions: `crypto_gcc_O2.exe:rc4_init@0x140001530`,
  `libc_types_gcc_O2.exe:count_spaces@0x1400014f0`, and
  `semantic_stress_clang_O2.exe:bounded_tlv_sum@0x140001590`.
- Corpus: external `/Users/sjkim1127/fission-benchmark`, `dev`, local Fission
  endpoint built from `5ec221602`, fingerprint
  `7a3104eddbb3b0bb674e021fa0218f23dc9a28cc9fe2a38db2f533434000280a`; all
  rows were run with `--no-resume`.
- Baseline artifacts:

  ```text
  /Users/sjkim1127/fission-benchmark/results/issue60_before_rc4_init.json
  /Users/sjkim1127/fission-benchmark/results/issue60_before_count_spaces.json
  /Users/sjkim1127/fission-benchmark/results/issue60_before_bounded_tlv_sum.json
  ```

- `rc4_init` has 9 compiler rows (24.44% mean semantic pass rate); GCC O2 is
  0/5 and the semantic harness fails on undeclared `xmm1_wh` at the assignment
  `xmm1_wh = (ushort)(xmm4_qa >> 16)`. `xmm1_wh` is absent from the function's
  local declaration list.
- `count_spaces` has 6 semantic compiler rows, all 0/6. GCC O2's bare compile
  reports undeclared `xmm0_qb`; the semantic harness also has an independent
  `strlen` prototype conflict, so that row cannot isolate the name failure.
- `bounded_tlv_sum` has 9 compiler rows (3 pass all 7 semantic cases); Clang
  O2 reports undeclared `xmm0_qa`. Other variants have independent pointer,
  runtime, and assertion failures.

The issue-specific evidence is the undeclared SIMD identifiers in the emitted
function C and compiler diagnostics, not the aggregate row score. These rows
also have independent failures; only the focused row deltas below are claimed,
not a headline or leaderboard change.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

`fission-midend-normalize::cleanup::temp_var::rescue_undeclared_bindings` is
the existing declaration-closure owner. It collects body names and creates a
local only when `is_rescue_candidate_name` accepts that spelling. The current
predicate covers builder temps, stack homes, flags, and names beginning with
`r`/`e`, but rejects valid lane names such as `xmm1_wh`, `xmm0_qb`, and
`xmm0_qa`. Consequently the final normalize rescue leaves those identifiers
undeclared; the renderer faithfully prints the missing binding as an ordinary
variable use/assignment.

The normalized NIR already contains the failing use, while the function's
local list does not:

```c
uint xmm0_qa;
int xmm1_qa;
/* no xmm1_wh declaration */
xmm1_wh = (ushort)(xmm4_qa >> 16);
```

The semantic harness confirms the first failure as `use of undeclared
identifier 'xmm1_wh'`. This is a missing local-declaration fact, not a C
formatting defect.

## 3. Generality / Invariant Proof

```text
When a body references an undeclared identifier that follows the established
SLEIGH SIMD register spelling `xmmN`, `ymmN`, or `zmmN`, optionally followed
by a lane suffix `_...`, the existing rescue pass must add one local binding
unless the name is already a parameter or local. Infer its type from its first
assignment exactly as for other rescued temporaries.
```

The predicate is based on the identifier grammar emitted by the register
namer, not on an ISA enum, binary, function, address, or compiler tuple. It
does not treat arbitrary names containing `xmm` as registers. A synthetic
test will cover multiple SIMD families, lane suffixes, inferred assignment
types, idempotence, and rejection of unrelated identifiers.

## 4. Risk And Ownership Check

- Existing pass: `rescue_undeclared_bindings` in `cleanup/temp_var.rs`.
- Shared analysis candidate: none; declaration closure already owns this
  narrow naming fallback and the issue is not reaching-definitions or type
  inference policy.
- Extending the existing predicate is sufficient: names already flow through
  the body collector and first-assignment type inference.
- Interaction: the rescue runs at multiple points in normalize cleanup. It
  adds a binding only if absent, so repeated invocation remains idempotent and
  does not change any already-declared register/local.
- New owner dependency: none.
- Telemetry: none.
- Known cases to preserve: existing `xVar`/`tmp_`/`local_`/flags and ordinary
  GPR fallback rescue; unknown function identifiers and unrelated C locals
  must not become guessed declarations.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize rescue_undeclared_bindings_declares_simd_lane_names`
  - Expected: previously undeclared SIMD lane locals are added with inferred
    types; arbitrary identifiers remain untouched.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize`
  - Expected: existing normalize behavior remains green.
- [x] Focused benchmark rows:
  - Command: rerun the three baseline `runner.py --corpus dev --function ...
    --decompilers fission --no-resume --run-mode local` commands.
  - After artifacts:

    ```text
    /Users/sjkim1127/fission-benchmark/results/issue60_after_rc4_init.json
    /Users/sjkim1127/fission-benchmark/results/issue60_after_count_spaces.json
    /Users/sjkim1127/fission-benchmark/results/issue60_after_bounded_tlv_sum.json
    ```

  - Bundle: local Fission build from `5ec221602` plus this working change,
    fingerprint `f88cbdab392b47cf608480a06b11174486b2eac03654f65a09a9cf87b43c5db9`;
    all three requests used `--no-resume`.
  - `rc4_init` GCC O2: undeclared `xmm1_wh` compile error is gone; the row now
    reaches an assertion failure (still 0/5 cases). The row mean remains
    24.44%.
  - `count_spaces` GCC O2 bare compile: `xmm0_qb` undeclared error is gone;
    an independent pointer-to-integer conversion remains. Its semantic harness
    still fails earlier on the independent `strlen` prototype conflict, so
    its semantic rate remains 0/6.
  - `bounded_tlv_sum` Clang O2: undeclared `xmm0_qa` compile error is gone;
    the row now reaches semantic execution and passes 3/7 cases. Across its
    eight semantically measured rows, mean pass rate changed from 0.4464 to
    0.5000 (three perfect rows in both runs). Other compiler variants retain
    independent failures.
  - These are focused real-row measurements only; no global score or ranking
    claim is made.
- [x] Smoke or automation sample:
  - `cargo nextest run -p fission-pcode --no-fail-fast`: 1113 passed, 3
    unrelated existing failures, 1 skipped. The same three failures were
    present in the pre-change full-suite baseline.
  - `CARGO_BUILD_JOBS=1 cargo build -p fission-cli --release`: passed.
  - `cargo check -p fission-pcode -p fission-decompiler`: passed.
  - `cargo fmt --all --check` and `git diff --check`: passed.
- [x] No new pass/helper/dependency; no boundary scan required.

## 6. AI Review / Prompt Firewall

- Was another AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Ghidra guidance: correctness only; no output-style request.
- Synthetic invariant test and the three real-row anchors are the validation
  evidence; the motivating rows are not a tuning target.

## 7. Review Notes

- Production code contains no binary/function/address/corpus guards:
  - [x] Confirmed
- No semantic-improvement claim will be made from synthetic tests or a
  benchmark score alone:
  - [x] Confirmed
- No new pass/helper duplicates an existing owner:
  - [x] Confirmed
