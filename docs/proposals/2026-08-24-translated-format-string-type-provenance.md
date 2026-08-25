# Preserve translated format-string type provenance

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_195.elf`
- Function: `sparse_offset_decoder`
- Address: `0x1fad8`
- Corpus row or benchmark command: cache-disabled 250-function sample-set NIR sweep plus the fixed 109-row upstream Types census.
- Current output summary: `gettext("Malformed extended header: excess %s=%s")` is assigned to `rax`, then passed as `error`'s format argument. The second `%s` argument is `local_28`, copied from `param_3`, but the function signature still declares the source `char *arg` as `ulonglong`.
- Semantic cases passed / total: no public per-row behavior oracle; the current external fixed-40 regression baseline is semantic-perfect 22/40 with mean 0.6442.
- Failure category: the printf format analyzer accepts only a literal directly at the formatting call. It discards a literal's specifier types when the same literal is first passed through the imported `gettext`/`dcgettext` identity and copied into a local.
- Relevant benchmark/static/readability observations: the current fixed-109 Types result is 6 perfect with total distance 1184; current NIR/HIR gotos are 1079/1077. A rendered-code census finds 57 translated literal-format callsites in 18 sample files and six projects, spanning O0, O2, and O2-noinline. Targets are `error`, `printf`/`fprintf`, and glibc fortified printf entry points.

Comparable real rows:

- `bin_039.elf` / `copy_reg` / `0x8680` (coreutils O2-noinline): repeated translated `%s` diagnostics consume source/destination names still declared as integer types; Types distance 31.
- `bin_078.elf` / `describe_change` / `0x3630` (coreutils O2-noinline): translated one-, two-, and three-`%s` formats flow through `__printf_chk`; `file` and `old_group` remain integer declarations; Types distance 7.
- `bin_127.elf` / `dopass` / `0x3a20` (coreutils O2-noinline): translated `"%s: pass %lu/%lu (%s)..."` flows into `error`; source `qname` remains `longlong`; Types distance 31.
- `bin_040.elf` (dpkg O0), `bin_046.elf` (shadow O0), and `bin_148.elf` (diffutils O2) repeat the same translation-to-formatting chain outside the scored near-perfect rows.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize/type recovery
- [x] Signature/type resource ownership
- [ ] Structuring
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

The raw calls, literal addresses, assignments, and source argument values are already present. `callsite_type_prop.rs::apply_variadic_printf_format_string_arg_types` already owns parsing `%` conversions and propagating their types backward through copy aliases, but obtains format text only from a quoted literal used directly as the formatting call argument. The translated form is already an ordinary assignment in PreHIR, so no binary or printer repair is needed.

The existing signature tables do not describe GNU `error` or glibc fortified printf entries, and the packed format discards a C `...` marker. The signature owner therefore must provide their fixed parameter prefix and centrally classify them as variadic. The normalize owner can then require import provenance before admitting the new runtime-specific format indices.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Within one straight-line statement region, a quoted literal passed as the
message argument of an exact imported gettext/dcgettext call remains the
format contract of that call's result. If that result, or a plain-copy alias,
is then used as the format argument of an exact imported printf-style runtime
entry, parse the literal's conversions and apply the existing variadic
argument type constraints.

Kill the provenance when the receiving variable is overwritten, definitions
conflict, or control flow prevents one reaching value. Do not infer through an
unknown translation function, unknown formatting function, memory, a call
argument other than the documented message/format slot, or an internal symbol
that merely shares a runtime name.
```

- [x] Production admission uses typed import provenance, exact runtime contracts, statement order, variable definitions, and existing copy/type guards.
- [x] No ISA, compiler, function, address, binary, project, or corpus identity appears in the rule.
- [x] Evidence repeats across at least four projects and all three sample optimization families.
- [x] Direct literals retain the existing behavior; dynamic message IDs and ambiguous definitions remain negative cases.

The output is better without mentioning a metric because translating or copying a format string does not change the C types required by its conversion specifiers. Declaring a value consumed by `%s` as an integer contradicts the call contract already present in the function.

## 4. Risk And Ownership Check

- Existing owners: `fission-midend-normalize` owns callsite format typing; `fission-signatures` owns runtime declarations and variadic-family facts.
- Reuse the existing `NirType`, copy-chain refinement, `CallSummary.target.provenance`, and API signature transport. Do not add a pass or parallel fact map.
- Site sensitivity is required. A function-global map from `rax` to a format literal is unsafe because optimized functions reuse the same binding for many translated strings.
- Branches and loops may be visited for their local callsites, but translation provenance must not leak across a control-flow boundary unless both paths prove the same value.
- Runtime-specific new indices (`error`, `error_at_line`, fortified printf family) are admitted only for imports backed by the signature context. An internal function named `error` is not proof.
- Positional, width, precision, and conversion parsing retains the existing parser's conservative behavior. Unknown conversions do not refine a value.
- The existing type/surface conflict rules remain authoritative; this slice supplies format evidence but does not weaken type admission.

## 5. Validation Matrix

- [x] Relevant crate tests: `cargo nextest run -p fission-midend-normalize -p
      fission-signatures` -- 431 passed.
- [x] Release CLI builds; `rustfmt --check` clean on both changed sources (the
      50 hunks in `lib.rs` predate this and are untouched by its one-line
      export).
- [x] **Full sample regression on the real scored binaries.** Cache-disabled
      250-function NIR sweep of
      `vendor/decbench-evalkit/decbench-evalkit-sample-set`: **250/250**, gotos
      1,079 -> 1,081, short-circuit terms 220 unchanged. A type change should
      not move control flow, and it essentially does not.
- [x] **Anchor row.** `bin_195.elf` / `sub_1fad8`: `param_3` goes `ulonglong`
      -> `uchar *`, `local_28` follows it, and the translation result `rax`
      becomes `char*`. Exactly the chain this proposal describes.
- [x] **Direction census across the scored corpus.** 55 of 250 functions change
      output, 154 local declarations change type:

```text
integer -> pointer      78
pointer -> integer       4
other                   72

ulonglong   -> uchar *  37      uint *  -> char*     13
longlong    -> char*    16      int *   -> uint *    13
ulonglong * -> char*    12      char*   -> FILE*     10
```

      The 78:4 split is the claim: this recovers pointer-ness that was being
      printed as an integer. It reaches shadow (10), gnutls (8), coreutils (6),
      sysvinit (6), diffutils (5), libacl (4), dpkg (2) and tar (2), at
      `O2-noinline` (35), `O2` (15) and `O0` (5).

### What cannot be measured locally, and why

- [ ] **Types score on the scored corpus.** Not deferred -- *impossible here*.
      The evalkit's binaries carry neither DWARF nor a symbol table (checked:
      0 of 40), so there is no local ground truth to score against. DecBench
      scores from unstripped originals it does not distribute; the maintainer's
      merge notes say as much ("the kit carries no structured variable data").
      Only a submission will answer it.

- [x] **`corpus/dev` type metric: unchanged, and that is expected.** 305
      functions, perfect 88 and accuracy 0.555 before and after, identical.
      `corpus/dev` is small single-purpose programs with no translated
      diagnostics, so it contains none of this change's targets. **It must not
      be read as "the change does nothing"** -- the 55 functions that do move
      are all real projects. Recording this because the natural reading is the
      wrong one.

- [ ] External Docker regression, and the fixed-40/fixed-109 comparisons this
      proposal's earlier draft referenced. **No such harness exists in the
      repository** -- `runner/` has nothing matching, so those rows were
      planned rather than run. Either the harness lands with the claim or the
      claim goes.

## 6. AI Review / Prompt Firewall

- Was an external or cross-model implementation suggestion used?
  - [x] No
  - [ ] Yes, with benchmark identities redacted by the review template
- Row identities are confined to this evidence document. Production and synthetic tests will state only imported-call, format-slot, and reaching-definition invariants.
- Vendor implementations are not linked, invoked, or copied.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guard:
  - [x] Confirmed by design.
- The change is justified independently of Types distance:
  - [x] A `%s` argument is a character pointer even when its format literal was translated or copied first.
- Existing owner is extended rather than duplicated:
  - [x] The current callsite format analyzer and signature provider remain the only owners.
