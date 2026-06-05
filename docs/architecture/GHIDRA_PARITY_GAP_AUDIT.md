# Ghidra Parity Gap Audit

Updated: 2026-06-05

This document records Fission's current structural gaps against the Ghidra
owner chains used as reference. It is reporting-only: the audit must not repair
semantics, promote approximate P-code, or turn unsupported inputs into success.

Reference root:
`vendor/ghidra/ghidra-Ghidra_12.0.4_build`

## Owner Chains

| Area | Ghidra owner chain | Fission status |
|---|---|---|
| SLEIGH | `SleighLanguage -> SubtableSymbol -> DecisionNode -> Constructor -> ParserWalker/ConstructState -> HandleTpl.fix -> ConstructTpl -> PcodeEmit` | Partial. Checked-in `.sla` ConstructTpl overlay is the production success source (`SpecDerived` only). Slaspec lowering supplies metadata/layout; slaspec decision trees are not used on the lift path. `token.rs` reads SLA token fields; `BoundOperand` is display/debug-only. |
| Loader | `detect -> findSupportedLoadSpecs -> map memory blocks -> symbols/imports/exports -> finalize` | Partial. PE/COFF/ELF/Mach-O and several secondary loaders are Fission-owned; lower-priority Ghidra loader families remain typed unsupported. |
| FID | `DBHandle -> Table -> DBRecord -> FidDB -> FidHasher -> FidMatcher/FidProgramSeeker` | Partial. Raw `.fidbf` DBHandle records are decoded for installed databases, but packed `.fidb` and complete Ghidra-style program seeking/hash input coverage remain typed unsupported. |

## Current Audit Snapshot

Generated with:

```bash
python3 scripts/audit/ghidra_parity_audit.py --markdown
```

| Probe | Owner chain | Status | Evidence | Next action |
|---|---|---|---|---|
| `sleigh_native_model` | `SleighLanguage -> SubtableSymbol -> DecisionNode -> Constructor` | `implemented` | Checked-in `utils/sleigh-specs/compiled/` overlay required; slaspec throwaway subtable build removed | Keep `.sla` overlay byte-stable; extend compiled coverage for remaining Toy-only slaspec entries only when `.sla` is checked in. |
| `sleigh_token_cursor` | `ParserWalker` token field traversal | `partial` | SLA token field reads in `compiled_table/token.rs`; some pattern-expression paths still use instruction cursor context | Audit operand-absolute vs cursor-relative token reads; fail typed when SLA metadata is insufficient. |
| `sleigh_handle_resolution` | `HandleTpl.fix -> FixedHandle -> PcodeEmit` | `partial` | Exported/fixed handles from SLA ConstructTpl; BoundOperand retained for display/debug only | Row-level audit for any remaining handle resolution gaps on real `.sla` templates. |
| `sleigh_compatibility_sources` | `ConstructTpl` execution source | `implemented` | `CompiledTemplateSource::SpecDerived` only; native DLL and `CompatibilityLowered` paths removed | Guard tests must keep non-SpecDerived template sources out of runtime success paths. |
| `loader_family_matrix` | loader detect/probe/map/symbol/finalize | `partial` | Ghidra `*Loader.java` files found: 56; executable detected formats: PE, COFF, ELF, Mach-O, TE, Intel HEX, Motorola HEX, MZ, NE, Unix a.out | Keep a documented implemented/known-unsupported matrix and route only executable formats to `LoadedBinary`. |
| `loader_raw_binary` | `BinaryLoader` raw blob | `typed_unsupported` | `BinaryLoader` mentions: 5 | Keep raw binary opt-in only; unknown bytes remain `UnsupportedFormat` unless an explicit load hint is provided. |
| `loader_postload_analyzers` | post-load enrichment outside format owner | `implemented` | post-load analyzers are restricted to explicit binary evidence | Ensure Go/Rust/C++ enrichment output does not own format detection, load-spec selection, memory mapping, or default seeds. |
| `fid_raw_dbhandle` | `DBHandle -> Table -> DBRecord -> FidDB` | `partial` | Ghidra FID/DB reference files found: 5; raw DBHandle/table reader mentions: 14; packed DB typed unsupported mentions: 3 | Extend raw DBHandle coverage only with exact record/page decoding; packed `.fidb` remains typed unsupported until implemented. |
| `fid_hash_and_match` | `FidHasher -> FidMatcher -> FidProgramSeeker` | `partial` | `UnsupportedFidHashInput` mentions: 4; relation metadata mentions: 56 | Integrate exact instruction-mask and relation context before promoting matches to `StrongFid`; missing inputs remain typed unsupported. |

## Closure Rules

- Raw P-code success must report only real decoded `.sla ConstructTpl` source (`SpecDerived`).
- `BoundOperand` and display helpers may be used for operand text, but must not become pcode emit fallbacks.
- Loader container/unsupported inputs fail closed with typed errors and are not passed to raw P-code lanes.
- FID matches are `StrongFid` only when they come from decoded database records and exact hash inputs.
- Coverage regressions are acceptable only when the replacement is typed unsupported rather than approximate success.
