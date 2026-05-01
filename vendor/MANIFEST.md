# `vendor/` manifest

**Last verified:** 2026-05-02

The [`vendor/`](../vendor/) directory contains **third-party source and reference trees**. It exists for **comparison, invariants, and build-time FFI/data** — not as a place to grow Fission-specific business logic.

## Roots (summary)

| Path | Contents | Typical use |
|------|----------|-------------|
| [`ghidra/`](./ghidra/) | Ghidra release / build extracts | Reference for Sleigh, decompiler behavior, file formats |
| [`retdec-5.0/`](./retdec-5.0/) | RetDec sources (MIT) | Reference algorithms and regression thought experiments |
| [`libsla/`](./libsla/), [`libsla-sys/`](./libsla-sys/) | Sleigh library and sys crate | Build-time / runtime linkage per `docs/build/BUILD.md` |
| [`binaries/`](./binaries/) | Small test/reference binaries with per-file licenses | Controlled test inputs (review license headers) |

## Rules

1. **Do not** copy vendor code into `crates/fission-*` wholesale. Prefer thin adapters and cite upstream in PRs when porting ideas.
2. **Reference-only** trees (Ghidra UI, RetDec monolith) must not become hidden runtime dependencies without an ADR and release checklist update ([`docs/RELEASE.md`](../docs/RELEASE.md)).
3. License detail and update steps: [`THIRD_PARTY.md`](../THIRD_PARTY.md).
