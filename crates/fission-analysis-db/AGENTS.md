# Fission Analysis Database Guide

Scope: `crates/fission-analysis-db/`

This crate owns the typed, immutable program-wide metadata view consumed by
analysis and decompiler layers. The loader remains the owner of format parsing;
this crate canonicalizes loader facts into deterministic IDs, provenance, and
cross-table references.

## Rules

- Depend on loader facts; never parse binary formats here.
- Keep snapshot construction deterministic and query surfaces read-only.
- Add a new fact only with an explicit source, confidence, and canonical owner.
- Do not put NIR transformations, CFG structuring, or printer policy here.
- Future analysis updates must be expressed as typed deltas, not mutation of
  loader maps or unrelated debug-information structures.
