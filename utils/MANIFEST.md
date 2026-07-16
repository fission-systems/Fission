# `utils/` manifest

**Last verified:** 2026-07-15

The [`utils/`](../utils/) tree holds **checked-in data and specs** used by builds, tests, and tooling. It is not a dumping ground for one-off binaries (see [`docs/MALWARE_SAMPLE_POLICY.md`](../docs/MALWARE_SAMPLE_POLICY.md)).

## Release packaging

- **SemVer releases** (`cd.yml`): full platform archives embed `utils/`; the same tag also publishes standalone **`fission-utils.tar.gz`**.
- **CI bootstrap**: [`.github/actions/setup-utils`](../.github/actions/setup-utils) downloads `fission-utils.tar.gz` from the long-lived **`assets-v1`** (or configured) release tag.
- **Refresh `assets-v*`**: Actions → **Publish Utils Assets** ([`publish-utils-assets.yml`](../.github/workflows/publish-utils-assets.yml)).
- Policy detail: [`docs/CI_RELEASE_GATES.md`](../docs/CI_RELEASE_GATES.md) § Resource bundle.

## Major subtrees

| Path | Role |
|------|------|
| [`ghidra-data/`](./ghidra-data/) | Ghidra-derived opinion files and related packaged data (see [`NOTICE`](./ghidra-data/NOTICE)). Detailed provenance: [`THIRD_PARTY.md`](../THIRD_PARTY.md). |
| [`signatures/die/detect-it-easy/`](./signatures/die/detect-it-easy/) | Detect It Easy signature corpus used by detector integrations. License: bundled MIT `LICENSE`; upstream horsicq/DiE. |
| [`signatures/typeinfo/win32/`](./signatures/typeinfo/win32/) | Canonical Win32 API/type surface: `win_api_signatures.txt`, `base_types.json`, `structures.json`, GDT/type supplements used by [`crates/fission-signatures`](../crates/fission-signatures). |
| [`signatures/patterns/`](./signatures/patterns/) | MSVC/CRT byte-pattern corpora (e.g. `msvc_x64_crt.json`) for [`SignatureDatabase`](../crates/fission-signatures/src/database.rs). |
| [`signatures/fid/`](./signatures/fid/) | FIDBF/FIDB assets consumed by FID loaders in [`crates/fission-signatures`](../crates/fission-signatures). |
| Other paths under [`signatures/`](./signatures/) | Additional signature assets — document sizable imports here; [`crates/fission-signatures`](../crates/fission-signatures) must not duplicate them under `data/`. |

## Conventions

- Prefer **small, verifiable fixtures** over large opaque blobs.
- When importing upstream dumps, record **upstream URL + tag/commit** in [`THIRD_PARTY.md`](../THIRD_PARTY.md) and bump **Last verified** here.
- For DIE primitive gaps or unsupported detectors, coordinate metric naming with [`docs/QUALITY_METRICS.md`](../docs/QUALITY_METRICS.md) rather than inventing parallel JSON schemas.
