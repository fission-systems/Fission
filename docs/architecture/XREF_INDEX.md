# Canonical XrefIndex

This note captures **ownership**, **surface APIs**, and the implemented evidence layers for the canonical cross-reference index. Implementation detail lives in Rust (`crates/fission-static/src/analysis/xref_index/`).

## Ownership

- **`fission-static`** owns [`xref_index`](../../crates/fission-static/src/analysis/xref_index/mod.rs): `XrefRecord`, `XrefIndexBuilder`, merge helpers, and queries (`refs_from_address`, `refs_to_address`, summaries).
- **`fission-cli`** consumes the public API only (no duplicated decode logic).

## Layers today

| Layer | Source | Notes |
|-------|--------|--------|
| Loader | `LoadedBinary` (`iat_symbols`, exports, `string_map`, `global_symbols`) | High confidence where symbols are authoritative |
| Disassembly | [`XrefDatabase`](../../crates/fission-static/src/analysis/xrefs/mod.rs) built via `RuntimeSleighFrontend::decode_window` | Operand references + decoded flow targets; merged as `XrefSourceLayer::Disassembly` |
| Relocation | `LoadedBinary::relocations` plus legacy `relocation_symbols` | Structured entries preserve raw type, size, addend, and optional symbol; symbol-use-site entries without a matching table row remain supported. Empty symbol names are represented as unresolved, never as empty target symbols. |
| P-code | Bounded per-function lift plus value-set facts and direct RAM-space varnodes | Emits only mapped data targets or executable/import flow targets; evidence records the p-code opcode. Function bytes are file-backed, capped at 1 MiB and 4,096 instructions, and must lift to terminal control flow. |

Confidence follows [`fission_loader::Confidence`](../../crates/fission-loader/src/detector/mod.rs); Low-confidence facts remain eligible for omission from downstream “confirmed” surfaces.

## CLI surfaces

- `fission_cli xrefs <binary> [--json] [--no-disassembly] [--pcode] [--function ADDR]` emits the merged index (full `refs` in JSON). P-code analysis is opt-in because it adds a bounded lift over known functions.
- `fission_cli info <binary> --xrefs [--json]` embeds the summary and analysis coverage under `xrefs` without dumping every record.

## Analysis coverage contract

The `xrefs` JSON result includes `analysis.layers`; the `--to` result includes the same report, including when it finds no records. `info --xrefs` includes it beside the summary. Each layer reports its state, bounded scope, candidate/completed/omitted input-unit counts, emitted records, and typed omission counts.

States mean:

- `not_requested`: the caller disabled this layer.
- `complete_for_scope`: every candidate unit in the layer's documented scope was processed. This is not a claim that every possible binary reference was found.
- `partial`: the layer started, but one or more candidate units were omitted. This includes a run where every candidate was omitted; `completed_units` reports whether any work completed.
- `unsupported`: a layer-level prerequisite prevented analysis, for example because the binary has no load spec or the Sleigh frontend/RAM space is unavailable. `unsupported_reason` gives the prerequisite failure separately from per-candidate `omissions`.
- `failed`: reserved for a layer that reports an analysis failure distinct from unsupported inputs.

`candidate_units`, `completed_units`, and `omitted_units` count the layer's declared unit type. Each value in `omissions` counts candidate units skipped after layer setup; layer-level prerequisite failures appear in `unsupported_reason` and do not fabricate an omission count when there were no candidates.

Loader, symbol-table, and relocation layers are complete only for facts already present in `LoadedBinary`. Disassembly coverage is limited to file-backed executable sections decoded linearly and pointer-sized aligned slots in readable, non-executable file-backed sections. Every discovered non-import function is a p-code candidate. A candidate completes only when it has a nonzero, complete file-backed extent of at most 1 MiB, reaches terminal control flow within 4,096 decoded instructions, and passes value-set analysis; candidates that miss any requirement stay in the candidate count and increment a typed omission reason. An empty `refs` list or absent `by_layer` entry must therefore be interpreted together with the corresponding coverage state; it is not by itself proof that no references exist.

## P-code evidence contract

P-code evidence comes from two sources:

- bounded value-set analysis for constant `LOAD`/`STORE` addresses and resolved indirect branches/calls;
- direct RAM-space varnodes in raw p-code, including absolute memory operands represented as `COPY` rather than `LOAD`/`STORE`.

RAM identity and addressable-unit size come from the compiled Sleigh language metadata, not an ISA-specific numeric address-space ID. A p-code flow edge is promoted only when the decoded machine instruction at the same address has the corresponding call/jump classification, and sequential fall-through edges are omitted. Direct flow targets must resolve to executable memory or a known import slot; data targets must fall within a mapped section. Unknown values and incomplete/oversized lifts are omitted. Separate p-code evidence remains distinguishable from disassembly evidence even when both establish the same source/target pair.

The legacy [`XrefDatabase::refine_with_vsa`](../../crates/fission-static/src/analysis/xrefs/mod.rs) API follows the same file-backed function-slice and terminal-lift bounds; virtual addresses are never treated as file offsets.

## Benchmark / oracle alignment

Stage parity JSON adds `stages.xrefs` counters via [`benchmark/stage_parity_benchmark/stage_metrics.py`](../../benchmark/stage_parity_benchmark/stage_metrics.py). Rows may populate top-level `xref_metrics`; Ghidra joins remain under [`benchmark/ghidra_oracle_benchmark/`](../../benchmark/ghidra_oracle_benchmark/README.md). For a direct local comparison, [`scripts/test/GhidraXrefExport.java`](../../scripts/test/GhidraXrefExport.java) exports instruction-origin references as source/target pairs.
