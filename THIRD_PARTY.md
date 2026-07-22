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
| Per-architecture DWARF register mapping tables (`*.dwarf`) | `utils/sleigh-specs/languages/<Arch>/*.dwarf` (gitignored local/CI asset tree, not committed to this repo -- see [`utils/MANIFEST.md`](./utils/MANIFEST.md) § Release packaging) | `Ghidra/Processors/<Arch>/data/languages/*.dwarf` in [`vendor/ghidra/`](./vendor/ghidra/), Ghidra 12.0.4 | Apache-2.0 (see vendor `LICENSE` trees) | **Reference data, not yet runtime-consumed**: maps DWARF register numbers to Ghidra register names per architecture (19 files, ~76K, copied 2026-07-20). No architecture ships one for LoongArch as of Ghidra 12.0.4 -- would need the LoongArch psABI spec directly if that mapping is ever needed. Intended input for a future DWARF-register-resident-local naming feature (see `PROJECT.md`); not wired into any Rust code yet, so `utils/sleigh-specs/MANIFEST.sha256.json`/`ghidra_language_manifest.json` were deliberately left unregenerated -- neither is read by anything that would notice the new files until that feature lands. |

**Update procedure:** Replace the corresponding subtree with a fresh upstream extract; run the project’s documented validation (`cargo check` / targeted tests / smoke paths in [`docs/RELEASE.md`](docs/RELEASE.md)). Update **Last verified** dates in this file and [`utils/MANIFEST.md`](./utils/MANIFEST.md).

**Local modifications:** If you must fork a sleigh `.slaspec` or data file, keep the diff minimal and cite the upstream version in the PR.

---

## RetDec (reference)

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| RetDec 5.0 sources | [`vendor/retdec-5.0/`](./vendor/retdec-5.0/) | [RetDec](https://github.com/avast/retdec) snapshot | MIT (see [`vendor/retdec-5.0/LICENSE`](./vendor/retdec-5.0/LICENSE)) | **Reference-only** — invariants and comparison, not vendored into the Rust release binary as a submodule. |

Do **not** copy RetDec logic into production paths to “paper over” semantic gaps; follow [`AGENTS.md`](./AGENTS.md) ownership rules.

---

## Cranelift (reference, version-pinned to the live dependency)

| Item | Location | Upstream | License (summary) | Runtime vs reference |
|------|-----------|----------|-------------------|----------------------|
| `cranelift-{codegen,frontend,jit,module,native,entity,bforest,isle,codegen-meta,codegen-shared,control,bitset,srcgen,assembler-x64,assembler-x64-meta}` 0.133.1 sources | [`vendor/cranelift-0.133.1/`](./vendor/cranelift-0.133.1/) | [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) (part of the `bytecodealliance/wasmtime` repo), copied from the exact crates.io source tree already resolved by `Cargo.lock` | Apache-2.0 WITH LLVM-exception (see each subdirectory's own `LICENSE`) | **Reference-only** — `fission-emulator`'s actual Cranelift dependency still comes from crates.io via `crates/fission-emulator/Cargo.toml` (`cranelift-jit`/`-module`/`-codegen`/`-frontend`/`-native` `= "0.133.1"`) as normal; this tree exists to read Cranelift's own IR/codegen semantics directly while building `fission-emulator::selfjit`, a native JIT scaffold whose explicit end goal (see `selfjit/mod.rs`'s own module doc) is eventually replacing that dependency. Not linked into any build. |

**Update procedure:** if `Cargo.lock` bumps `cranelift-codegen`'s version, re-copy the matching version's source from `~/.cargo/registry/src/index.crates.io-*/cranelift-*-<version>/` (guaranteed to be the exact tree the workspace actually compiles against) rather than a wasmtime git tag -- wasmtime's own release tags use a unified `vN.N.N` scheme that doesn't line up 1:1 with `cranelift-codegen`'s own crates.io version numbers.

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
