# Third-party material and provenance

**Last verified:** 2026-05-02

This document satisfies the practical intent of [`CLA.md`](./CLA.md) § “Third-Party Material”: clearly identify bundled third-party trees, their licenses, whether they ship in release artifacts, and how to refresh them.

Normative architecture remains [`docs/architecture/ARCHITECTURE.md`](docs/architecture/ARCHITECTURE.md). This file focuses on **license, source, scope, and update procedure**.

## How to contribute third-party updates

1. Prefer **upstream drops** over hand-edited vendor trees.
2. Document **version or snapshot date**, **license**, and **runtime vs reference-only** in this file (or linked manifest).
3. If you patch vendored sources, describe the patch (file list + rationale) in the PR and add a pointer under the block below.

---

## Ghidra distribution and Ghidra-derived data

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| Ghidra sources / build drops | [`vendor/ghidra/`](./vendor/ghidra/) | [Ghidra releases](https://github.com/NationalSecurityAgency/ghidra/releases) | Apache-2.0 (see vendor `LICENSE` trees) | **Reference** for algorithms, file formats, and Sleigh semantics; not linked as a whole into the Rust workspace. |
| Bundled Ghidra opinion / specs (checked-in) | [`utils/ghidra-data/`](./utils/ghidra-data/) | Derived from Ghidra releases; see [`utils/ghidra-data/NOTICE`](./utils/ghidra-data/NOTICE) | Apache-2.0 (see NOTICE) | **Runtime data** consumed by loaders / sleigh / tooling as configured in this repo. |

**Update procedure:** Replace the corresponding subtree with a fresh upstream extract; run the project’s documented validation (`cargo check` / targeted tests / smoke paths in [`docs/RELEASE.md`](docs/RELEASE.md)). Update **Last verified** dates in this file and [`utils/MANIFEST.md`](./utils/MANIFEST.md).

**Local modifications:** If you must fork a sleigh `.slaspec` or data file, keep the diff minimal and cite the upstream version in the PR.

---

## RetDec (reference)

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| RetDec 5.0 sources | [`vendor/retdec-5.0/`](./vendor/retdec-5.0/) | [RetDec](https://github.com/avast/retdec) snapshot | MIT (see [`vendor/retdec-5.0/LICENSE`](./vendor/retdec-5.0/LICENSE)) | **Reference-only** — invariants and comparison, not vendored into the Rust release binary as a submodule. |

Do **not** copy RetDec logic into production paths to “paper over” semantic gaps; follow [`AGENTS.md`](./AGENTS.md) ownership rules.

---

## Detect It Easy (signatures)

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| DiE database / rules (subtree) | [`utils/signatures/die/detect-it-easy/`](./utils/signatures/die/detect-it-easy/) | [Detect It Easy](https://github.com/horsicq/Detect-It-Easy) | MIT (see bundled [`LICENSE`](./utils/signatures/die/detect-it-easy/LICENSE)) | **Runtime detector resource** where integrated; treat as versioned corpus data. |

**Update procedure:** Refresh the subtree from upstream, re-run signature/die integration tests if present, and note the upstream tag or commit in the PR.

---

## Sleigh shared library (libsla)

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| `libsla` / `libsla-sys` vendor stubs | [`vendor/libsla`](./vendor/libsla), [`vendor/libsla-sys`](./vendor/libsla-sys) | Ghidra-licensed components / FFI bindings as documented in-tree | See respective `LICENSE` files under those directories | **Build/runtime** linkage surface — follow [`docs/build/BUILD.md`](docs/build/BUILD.md). |

---

## Other crates.io / npm dependencies

Rust crates are declared in workspace `Cargo.toml` files; npm packages for the desktop UI live under [`crates/fission-tauri/package-lock.json`](./crates/fission-tauri/package-lock.json). Automated update proposals may arrive via [Dependabot](.github/dependabot.yml); release hygiene is described in [`docs/RELEASE.md`](docs/RELEASE.md).

---

## Manifest companions

- [`utils/MANIFEST.md`](./utils/MANIFEST.md) — role of each major `utils/` subtree.
- [`vendor/MANIFEST.md`](./vendor/MANIFEST.md) — vendor roots and “reference-only” boundaries.
