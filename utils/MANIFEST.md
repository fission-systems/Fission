# `utils/` manifest

**Last verified:** 2026-05-02

The [`utils/`](../utils/) tree holds **checked-in data and specs** used by builds, tests, and tooling. It is not a dumping ground for one-off binaries (see [`docs/MALWARE_SAMPLE_POLICY.md`](../docs/MALWARE_SAMPLE_POLICY.md)).

## Major subtrees

| Path | Role |
|------|------|
| [`ghidra-data/`](./ghidra-data/) | Ghidra-derived opinion files and related packaged data (see [`NOTICE`](./ghidra-data/NOTICE)). Detailed provenance: [`THIRD_PARTY.md`](../THIRD_PARTY.md). |
| [`signatures/die/detect-it-easy/`](./signatures/die/detect-it-easy/) | Detect It Easy signature corpus used by detector integrations. License: bundled MIT `LICENSE`; upstream horsicq/DiE. |
| Other paths under [`signatures/`](./signatures/) | Supplementary signature assets — prefer documenting new roots here when adding sizable corpora. |

## Conventions

- Prefer **small, verifiable fixtures** over large opaque blobs.
- When importing upstream dumps, record **upstream URL + tag/commit** in [`THIRD_PARTY.md`](../THIRD_PARTY.md) and bump **Last verified** here.
- For DIE primitive gaps or unsupported detectors, coordinate metric naming with [`docs/QUALITY_METRICS.md`](../docs/QUALITY_METRICS.md) rather than inventing parallel JSON schemas.
