# PE/COFF Symbol Inventory Completeness

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O0.exe`
- Surface: external metadata-parity `/metadata` comparison
- Current behavior: function-entry recall is 85/85 and relocation-address recall is 45/48, but symbol-address recall is low because Fission exposes 194 symbol rows (158 unique addresses) while Ghidra exposes 1,054 rows.
- Comparable loader surface: Ghidra has 569 unique `source=imported` symbol addresses. Fission currently matches 155. The PE COFF symbol table itself accounts for 568 of those addresses.
- Contract observation: the remaining Ghidra `default` and `analysis` symbols are generated header-field, branch, and switch labels and must be reported separately from loader-symbol recall.

## 2. Owner Proof

- [x] Loader / typed program metadata
- [x] Benchmark comparison contract

Evidence:

```text
PE `parse_coff_data_symbols` accepts defined C_EXT/C_STAT entries but then
`should_collect_global_symbol` retains only names containing `refptr` or
starting with `__imp_`. The canonical analysis-db snapshot therefore never
sees most section, static-data, and local-code-label facts already present in
the binary. Putting every COFF label into `global_symbols` would misclassify
code labels as global data, so the loader needs a separate typed symbol-table
inventory consumed by `fission-analysis-db`.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Every named, defined, non-auxiliary COFF symbol whose section index resolves to
a mapped PE section is a loader-owned symbol fact. Function symbols remain in
the canonical function inventory; other entries are classified as code labels
or data from section permissions. Undefined, absolute/debug, malformed, and
out-of-range symbols remain excluded.
```

- No binary, address, compiler, or corpus identity enters production code.
- The rule follows documented COFF storage, section, and type fields.
- `global_symbols` remains the decompiler's data-symbol map; the new typed
  inventory belongs to `LoadedBinary` and flows into `ProgramSnapshot`.

## 4. Risk And Ownership Check

- Existing owners: PE COFF parser, `LoadedBinary`, and `fission-analysis-db::build_symbols`.
- No analysis-generated Ghidra labels are invented in the loader.
- Risk: auxiliary records or undefined/debug symbols becoming bogus addresses.
  Guard by skipping aux records and requiring a positive, in-range section.
- Risk: duplicate function rows. Guard by leaving typed function entries in the
  existing function path and collecting only non-function symbols here.

## 5. Validation Matrix

- [x] PE COFF parser positive/negative unit tests.
- [x] `cargo nextest run -p fission-loader` (124/124 pass).
- [x] `cargo nextest run -p fission-analysis-db` (8/8 pass).
- [x] `cargo check -p fission-cli`.
- [x] Same-binary `/metadata` rerun: comparable loader-symbol recall changes from 155/569 (27.2%) to 568/569 (99.82%); candidate precision is 97.43%.
- [x] Metadata comparison contract reports loader symbols separately from
  Ghidra-generated default/analysis labels.
- [x] External metadata parity 20-row sample rerun: the 18 PE rows match 9,850/9,867 comparable loader addresses (99.826% mean recall, 97.233% mean precision). The ELF and AArch64 rows expose a separate cross-format source/provenance contract gap.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation review was requested.
- The production invariant is expressed only in COFF section/type/storage terms.

## 7. Review Notes

- [x] No hardcoded row identity.
- [x] No presentation-layer semantic patch.
- [x] The measured baseline precedes production changes.
