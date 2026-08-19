# `utils/` manifest

**Last verified:** 2026-08-19

The [`utils/`](../utils/) tree holds **checked-in data and specs** used by builds, tests, and tooling. It is not a dumping ground for one-off binaries (see [`docs/MALWARE_SAMPLE_POLICY.md`](../docs/MALWARE_SAMPLE_POLICY.md)).

## Packed, not raw

Everything the decompiler loads at runtime is a **`.fpk`** — a sorted-record container with a sparse first-key index over independently compressed blocks (see [`crates/fission-signatures/src/fpk.rs`](../crates/fission-signatures/src/fpk.rs)). The upstream files those are built from live under [`source/`](./source/), which is **gitignored**: it is 491M and nothing reads it at runtime.

That split is what makes this tree committable at all — 156M tracked instead of ~570M — and committing it is the point. The previous arrangement downloaded a `fission-utils.tar.gz` bundle from an `assets-v*` release tag; that bundle silently diverged from the tree it claimed to mirror, and no one could see it because git did not track either side.

**Regenerating a `.fpk` requires `source/`.** Repopulate it from the release bundle or the upstream projects in [`THIRD_PARTY.md`](../THIRD_PARTY.md), run the relevant packer in [`scripts/`](../scripts) (or `cargo run --bin pack_fid`), and commit the resulting `.fpk`.

## Release packaging

- **SemVer releases** (`cd.yml`): platform archives embed `utils/`. Nothing is published separately -- the tree reaches consumers through the clone or through the archive.
- **CI**: [`.github/actions/setup-utils`](../.github/actions/setup-utils) downloads nothing. It verifies the checkout — slaspec, `.fpk`, and DIE `.sg` counts, one per pipeline area, so a partially-missing tree fails loudly instead of degrading one capability in silence.
- Policy detail: [`docs/CI_RELEASE_GATES.md`](../docs/CI_RELEASE_GATES.md) § Resource bundle.

## Major subtrees

| Path | Size | Role |
|------|------|------|
| [`sleigh-specs/`](./sleigh-specs/) | 26M | Sleigh processor specs (146 `.slaspec`) consumed by [`crates/fission-sleigh`](../crates/fission-sleigh). Not packed — the compiler reads them as text. |
| [`signatures/fid/`](./signatures/fid/) | 65M | 228 Ghidra Function ID databases, one `.fpk` each, loaded lazily by [`fidbf/fpk_store.rs`](../crates/fission-signatures/src/fidbf/fpk_store.rs). The largest subtree; each DB is compressed independently, so cross-DB redundancy is still on the table. |
| [`signatures/typeinfo/`](./signatures/typeinfo/) | 45M | Library type surface: Win32 API signatures and structs (from GDT), Go type info per `GOOS`/`GOARCH`, macOS and generic supplements. |
| [`signatures/die/`](./signatures/die/) | 13M | Detect It Easy signature corpus (2,066 `.sg`) used by detector integrations. License: bundled MIT `LICENSE`; upstream horsicq/DiE. |
| [`signatures/ordinals/`](./signatures/ordinals/) | 5.4M | DLL export-ordinal → name tables, packed per architecture. |
| [`signatures/patterns/`](./signatures/patterns/) | 212K | MSVC/CRT byte-pattern corpora for [`SignatureDatabase`](../crates/fission-signatures/src/database.rs). |
| [`ghidra-data/`](./ghidra-data/) | 1.2M | Ghidra-derived opinion files and `.exports` lists (see [`NOTICE`](./ghidra-data/NOTICE)). Provenance: [`THIRD_PARTY.md`](../THIRD_PARTY.md). |
| `source/` | 491M | **Gitignored.** Raw upstream inputs (FIDB Java dumps, GDT archives, Ghidra exports) kept only to rebuild the packed artifacts above. |

Directory names are resolved in one place — [`crates/fission-core/src/core/resource_layout.rs`](../crates/fission-core/src/core/resource_layout.rs) — so moving a subtree is one edit, not a grep.

## Conventions

- Prefer **small, verifiable fixtures** over large opaque blobs.
- New runtime data arrives as a `.fpk` with its inputs under `source/`, never as a loose corpus.
- When importing upstream dumps, record **upstream URL + tag/commit** in [`THIRD_PARTY.md`](../THIRD_PARTY.md) and bump **Last verified** here.
- For DIE primitive gaps or unsupported detectors, coordinate metric naming with [`docs/QUALITY_METRICS.md`](../docs/QUALITY_METRICS.md) rather than inventing parallel JSON schemas.
