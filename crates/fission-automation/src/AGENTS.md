# Automation Reporting Guide

Generated: 2026-03-27
Scope: `crates/fission-automation/src/`

## Overview

This tree owns quality-lane execution, summary generation, deltas, and go/stop signals for decompiler progress.

## Structure

```text
src/
├── main.rs       # CLI entry for automation lanes
├── report.rs     # Summary, delta, markdown/JSON reporting
├── diagnosis.rs  # Diagnosis buckets and next-patch signals
├── inventory.rs  # Corpus/inventory collection
├── lanes.rs      # Lane orchestration
├── corpus.rs     # Corpus model + artifact paths
└── model.rs      # Shared report model types
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Build stat export | `report.rs` | Must stay aligned with `NirBuildStats` |
| Lane execution | `lanes.rs`, `main.rs` | `nir-check` entry and profiles |
| Diagnosis buckets | `diagnosis.rs` | `next_patch` / dominant diagnosis |
| Inventory rows | `inventory.rs`, `model.rs` | Source row snapshots and aggregates |

## Conventions

- Treat `fission_pcode::NirBuildStats` as the only source for NIR build counters.
- When adding a new counter, wire it through report pair generation and summary output in the same change.
- Large-sample lane runs (200 / 500) are the real decision surface; avoid small-sample-only conclusions.

## Anti-Patterns

- Do not redefine a pcode metric locally with different semantics.
- Do not add reporting-only counters with no source in `NirBuildStats`.
- Do not change go/stop or summary interpretations without checking sample outputs.

## Validation

```bash
cargo check -p fission-automation
cargo run -p fission-automation -- nir-check --lane nir
```
