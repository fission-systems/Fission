# Ghidra Parity Gap Audit

Updated: 2026-05-01

This document records Fission's current structural gaps against the Ghidra
owner chains used as reference. It is reporting-only: the audit must not repair
semantics, promote approximate P-code, or turn unsupported inputs into success.

Reference root:
`vendor/ghidra/ghidra-Ghidra_12.0.4_build`

## Owner Chains

| Area | Ghidra owner chain | Fission status |
|---|---|---|
| SLEIGH | `SleighLanguage -> SubtableSymbol -> DecisionNode -> Constructor -> ParserWalker/ConstructState -> HandleTpl.fix -> ConstructTpl -> PcodeEmit` | Partial. Real `.sla ConstructTpl` execution is the success source, but legacy token cursor and BoundOperand-derived handle fallbacks still participate in some successful rows. |
| Loader | `detect -> findSupportedLoadSpecs -> map memory blocks -> symbols/imports/exports -> finalize` | Partial. PE/COFF/ELF/Mach-O and several secondary loaders are Fission-owned; lower-priority Ghidra loader families remain typed unsupported. |
| FID | `DBHandle -> Table -> DBRecord -> FidDB -> FidHasher -> FidMatcher/FidProgramSeeker` | Partial. Raw `.fidbf` DBHandle records are decoded for installed databases, but packed `.fidb` and complete Ghidra-style program seeking/hash input coverage remain typed unsupported. |

## Current Audit Snapshot

Generated with:

```bash
python3 scripts/audit/ghidra_parity_audit.py --markdown
```

| Probe | Owner chain | Status | Evidence | Next action |
|---|---|---|---|---|
| `sleigh_native_model` | `SleighLanguage -> SubtableSymbol -> DecisionNode -> Constructor` | `partial` | Ghidra reference files found: 5; Fission `SlaLanguage` mentions: 7; Fission `ConstructTpl` mentions: 9 | Promote `.sla` native identity to generated artifact source of truth. |
| `sleigh_token_cursor` | `ParserWalker` token field traversal | `legacy_debt` | legacy token/direct parser mentions: 32 | Replace shared-token/direct parser debt with decoded `.sla` token and operand metadata; fail typed when absent. |
| `sleigh_handle_resolution` | `HandleTpl.fix -> FixedHandle -> PcodeEmit` | `legacy_debt` | BoundOperand/manual handle mentions: 20; no-export fallback mentions: 2 | Remove BoundOperand-derived fixed handles from raw P-code success path after row-level audit shows exact exported handle coverage. |
| `sleigh_compatibility_sources` | `ConstructTpl` execution source | `legacy_debt` | `CompatibilityLowered`/`NativeFission` mentions: 2; mnemonic construct-kind classifier mentions: 2 | Keep compatibility/display debt outside template execution and audit success rows for real `.sla ConstructTpl` source only. |
| `loader_family_matrix` | loader detect/probe/map/symbol/finalize | `partial` | Ghidra `*Loader.java` files found: 56; executable detected formats: PE, COFF, ELF, Mach-O, Intel HEX, Motorola HEX, MZ, NE, Unix a.out | Keep a documented implemented/known-unsupported matrix and route only executable formats to `LoadedBinary`. |
| `loader_raw_binary` | `BinaryLoader` raw blob | `typed_unsupported` | `BinaryLoader` mentions: 5 | Keep raw binary opt-in only; unknown bytes remain `UnsupportedFormat` unless an explicit load hint is provided. |
| `loader_postload_analyzers` | post-load enrichment outside format owner | `legacy_debt` | post-load heuristic mentions: 5 | Ensure Go/Rust/C++ enrichment output does not own format detection, load-spec selection, memory mapping, or default seeds. |
| `fid_raw_dbhandle` | `DBHandle -> Table -> DBRecord -> FidDB` | `partial` | Ghidra FID/DB reference files found: 5; raw DBHandle/table reader mentions: 14; packed DB typed unsupported mentions: 3 | Extend raw DBHandle coverage only with exact record/page decoding; packed `.fidb` remains typed unsupported until implemented. |
| `fid_hash_and_match` | `FidHasher -> FidMatcher -> FidProgramSeeker` | `partial` | `UnsupportedFidHashInput` mentions: 4; relation metadata mentions: 56 | Integrate exact instruction-mask and relation context before promoting matches to `StrongFid`; missing inputs remain typed unsupported. |

## Closure Rules

- Raw P-code success must report only real decoded `.sla ConstructTpl` source.
- `BoundOperand`, mnemonic classifiers, source-line/opprint mappings, and legacy token policies may be displayed or audited, but they must not become semantic owners.
- Loader container/unsupported inputs fail closed with typed errors and are not passed to raw P-code lanes.
- FID matches are `StrongFid` only when they come from decoded database records and exact hash inputs.
- Coverage regressions are acceptable only when the replacement is typed unsupported rather than approximate success.
