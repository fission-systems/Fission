# Fission Docs Index

This document is the main entry point for the public Fission documentation set.

The public repository is still an **early prototype** and some documents are still being cleaned up or translated. For the current public source of truth, start with the documents below.

## Start Here

1. [`README.md`](../README.md)
   - repository overview
   - current engine status
   - quick build and run paths
2. [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
   - current architecture
   - `legacy` vs `mlil-preview`
   - crate/layer ownership
3. [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)
   - current public change history
   - quality and performance milestones
4. [`docs/build/BUILD.md`](./build/BUILD.md)
   - platform-specific build instructions

## Current Source Of Truth

Use these documents as the primary public references:

- product/workspace overview: [`README.md`](../README.md)
- system architecture: [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
- feature summary: [`docs/FEATURES.md`](./FEATURES.md)
- medium-term direction: [`docs/ROADMAP.md`](./ROADMAP.md)
- public changelog: [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)
- checked-in benchmark summary: [`docs/benchmark/grand_finale_summary.md`](./benchmark/grand_finale_summary.md)

Current crate ownership guidance:

- `fission-static`: source of truth for static analysis / decompilation orchestration
- `fission-dynamic`: source of truth for debugging / runtime / plugin / unpacker work
- `fission-analysis`: compatibility facade, not the preferred home for new code

For the public repository, AI/product orchestration layers are currently outside the repository scope.

## Changelog Policy

- [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md) is the public-facing English changelog
- [`docs/changelog/CHANGELOG.ko.md`](./changelog/CHANGELOG.ko.md) is the archived detailed Korean historical record

## Folder Guide

### `docs/architecture`

- system structure and crate/layer ownership
- FFI boundaries
- decompiler pipeline responsibilities

Representative document:

- [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)

### `docs/build`

- platform-specific build/run guidance
- security advisories

Representative documents:

- [`docs/build/BUILD.md`](./build/BUILD.md)
- [`docs/build/SECURITY_ADVISORIES.md`](./build/SECURITY_ADVISORIES.md)

### `docs/changelog`

- release/change history
- quality and performance milestone records

Representative documents:

- [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)
- [`docs/changelog/CHANGELOG.ko.md`](./changelog/CHANGELOG.ko.md)

### `docs/benchmark`

- checked-in benchmark summaries
- benchmark policy and reproducibility notes

Representative documents:

- [`docs/benchmark/grand_finale_summary.md`](./benchmark/grand_finale_summary.md)
- [`docs/benchmark/grand_finale_summary.json`](./benchmark/grand_finale_summary.json)

### `docs/analysis`

- deeper analysis notes about the decompiler, postprocess, type propagation, and FID behavior
- experiment and validation notes

This folder should be treated as **internal analysis notes**, not source of truth. Many documents capture research, experiments, and intermediate reasoning and may lag behind the latest implementation state. Treat architecture, roadmap, and changelog documents as the final authority.

Representative documents:

- [`docs/analysis/README.md`](./analysis/README.md)
- [`docs/analysis/KNOWN_ISSUES.md`](./analysis/KNOWN_ISSUES.md)
- [`docs/analysis/PASS_SYSTEM.md`](./analysis/PASS_SYSTEM.md)
- [`docs/analysis/POSTPROCESS_MODULES.md`](./analysis/POSTPROCESS_MODULES.md)

### `docs/cli`

- CLI behavior and one-shot mode notes

Representative document:

- [`docs/cli/CLI_ONE_SHOT_MODE.md`](./cli/CLI_ONE_SHOT_MODE.md)

### `docs/gui`

- GUI-related notes

Important note:

- [`docs/gui/GUI_GUIDE.md`](./gui/GUI_GUIDE.md) documents an older egui-era UI
- the current product UI reference is the Tauri frontend plus the root README
- this folder should be treated as **historical / reference-only** unless a document explicitly says otherwise

### `docs/idea`

- idea sketches
- investigation notes
- long-term experiment directions

This folder should be treated as **internal idea notes**. It is not source of truth. Cross-check anything here against the architecture, roadmap, and changelog before implementing from it.

Representative document:

- [`docs/idea/README.md`](./idea/README.md)

### `docs/plan`

- external engine / integration planning notes

### `docs/plugins`

- plugin development notes

## Recommended Reading Order By Goal

### Understanding the repository for the first time

1. [`README.md`](../README.md)
2. [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
3. [`docs/FEATURES.md`](./FEATURES.md)

### Building and running

1. [`docs/build/BUILD.md`](./build/BUILD.md)
2. [`README.md`](../README.md)

### Working on decompiler quality

1. [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)
2. [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
3. [`docs/benchmark/grand_finale_summary.md`](./benchmark/grand_finale_summary.md)
4. [`docs/analysis/KNOWN_ISSUES.md`](./analysis/KNOWN_ISSUES.md)

### Looking for experimental directions

1. [`docs/ROADMAP.md`](./ROADMAP.md)
2. `docs/idea/*`
