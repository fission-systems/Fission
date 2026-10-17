# Compact cold completed-decode storage

Status: mechanical memory/performance experiment, before production implementation. No semantic, readability, score or ranking improvement claim.

## Measured baseline and owner

Base main 064a85747d2d1c6c0726464dd77456cb66a7e9a1 has the identical tree to PR #207 source 400dfbe022a884a19e58abadd2b0ba21aef29a41. Its immutable measured executable SHA256 is b2731b579df71ecd640d3026b388c25be15ccf0ddf6b56c4724817170b927749. Frozen qualification revision 07dd95dbe6214b8a78fc1e4a55961ab515986489: large six-request batch median 31.6041 s and 3,897,802,752 maximum RSS bytes; small medians 2.1588/0.9377/1.0196 s. Existing timings/output references are in benchmark/artifacts/performance/2026-10-15-decode/ and docs/validation/instruction-probe-performance.{md,json}.

FactStore's existing DecodeCache retains a full Arc<DecodedPcodeFunction> for every complete FID decode. This is correctness-relevant: a caller with a smaller decode budget can consume the complete result, and switch retry can use it as fallback. Eviction, sampling functions, weak-only caching or simply discarding FID decodes can change output/evidence. Keep all original keys and values available. No-return and FID have different memory contexts/window bounds, so this experiment does not share their whole-function decodes.

## Owner-native invariant

Change representation, not admission or availability. Store cold complete results in an opaque in-process varint binary encoding of the canonical DecodedPcodeFunction. Use only the existing fission-sleigh bincode/serde dependencies. Every field, enum, signed/high-bit value, optional mnemonic, instruction, ordered collection, byte length and inferred edge must round-trip exactly. Do not persist bytes, read externally supplied archives, add a format/CLI option, drop metadata, normalize values or depend on vendor code.

The codec belongs with the canonical runtime value in fission-sleigh. The session cache and address map remain in fission-static. Encoding must run outside the shared map mutex. Each entry retains a weak reference to the original live Arc; a lookup upgrades that exact Arc if possible, otherwise materializes the complete archived result. Serialize a lookup per entry with a local mutex and keep its resulting Arc strongly. Drop the packed bytes when the entry becomes hot, so an all-function consumer does not permanently retain both representations. Preserve first-insertion-wins, complete-only admission and shared-cache clone behavior. Encoding failure retains the original strong Arc. An internally invalid immutable archive is an invariant failure, never a silent cache miss or fabricated partial decode. Codec initialization is synchronous, with no Rayon, callbacks, I/O, recursive cache lookup or global fact-map lock held.

Consult Ghidra sleigh.cc DisassemblyCache lifecycle for ownership principles only; do not copy its address-only key or implementation. This change does not alter Fission's existing cache identity or attempt to repair unrelated cache contracts.

## Validation and acceptance

- Complete graph round-trip including all canonical fields, empty graphs, every stop reason, enum aliases, high pointer bits, signed constants, empty-vs-absent text and malformed private archive rejection.
- Cache lifetime/identity: cold storage releases the original strong Arc; lookup yields the exact value and becomes hot; live original identity is retained; concurrent readers share one materialization; clones share cache; first insertion wins; incomplete results remain uncached; packed bytes are released on materialization and encoding failure retains the original.
- Focused nextest -> complete relevant sleigh/static/pcode/decompiler/CLI suites -> crate checks/release CLI/workspace fmt/owner scan. No changes to the primary or dirty main checkout during implementation/testing.
- Freeze a separate candidate. Five alternating small pairs and three alternating large pairs against immutable main; measure wall/CPU/RSS with no own concurrent builds/tests/profilers. Keep existing rendering and FID worker policies and limits. Investigate a time regression instead of disguising it as progress.
- Fresh 250 stripped-function replay must preserve every non-time function field (same seven duration exclusions). Requested inputs and address sets are frozen; source/debug information is not passed to producers. Preserve repeated focused output determinism. No execution or ranking claim.
- Keep only a measured useful memory reduction with output preservation and acceptable bounded timing behavior; otherwise revert the experiment. A successful memory experiment does not establish Kuna-level speed or cross-analysis instruction reuse.

## Accepted local evidence

Seven focused tests and 1,838 related tests pass (five skipped), with crate checks, release CLI, formatting and owner boundaries passing. Three alternating large pairs show median RSS 3,724,197,888 -> 2,409,381,888 bytes (-35.3%), wall 32.4801 -> 29.9963 s (-7.6%), CPU 150.3190 -> 143.7246 s (-4.4%). Small five-pair median differences are within 0.3% and include slight increases on Windows/macOS; no small-batch gain is claimed. A fresh 250-function replay changes no non-time records. Querying all functions can still grow to the previous hot-IR memory cost. Pinned Kuna remains 3.00–11.53 times faster; no new no-return/FID sharing is implemented. See docs/validation/completed-decode-storage-performance.{md,json} for all limits, data and reproduction commands.
