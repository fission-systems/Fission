# Security Advisory Policy (Rust / Node)

Last updated: 2026-03-03

## Purpose

This document defines the dependency security-review policy used by Fission.

- Rust: `cargo deny check advisories`
- Node (Tauri frontend): `npm audit --audit-level=high`

## Current Baseline

- Rollup is pinned to `4.59.0` through `overrides` in `crates/fission-tauri/package.json`.
- Rust advisories use `deny.toml` `advisories.ignore` only for **documented no-fix ecosystem issues**.

## Why an Ignore Baseline Exists

The Tauri Linux runtime path depends on GTK3/WebKit-family libraries such as `gtk`, `gdk`, and `webkit2gtk`, where some upstream advisories are effectively **no safe upgrade available** cases.

Those issues do not always have an immediately patchable safe version, so the policy is:

1. Advisory checks must still run in CI every time.
2. The ignore list in `deny.toml` must be limited to documented no-fix cases.
3. New advisories must fail CI so they are triaged explicitly.
4. The ignore list must be re-reviewed every quarter, or when Tauri / wry has a major update.

## Ignore Policy

### Conditions for Adding an Ignore

- `cargo deny` explicitly reports `Solution: No safe upgrade is available!`
- or the issue cannot be resolved without an upstream migration path (for example GTK4 migration)

### Conditions for Removing an Ignore

- a safe upgrade version becomes available
- or the dependency/runtime path has been replaced with an alternative stack

## Mid-Term Linux Target Strategy

1. Evaluate the Tauri Linux chain (gtk3/webkit2gtk) separately from the core CLI / analysis chain
2. Keep the core path on a zero-tolerance advisory policy
3. Keep the GUI path on a documented baseline plus scheduled re-review until the GTK4 / alternative-runtime migration is complete

## Audit Commands

```bash
# Rust advisories
cargo deny check advisories

# Node advisories
cd crates/fission-tauri
npm ci --ignore-scripts
npm audit --audit-level=high
```

## Related Files

- `deny.toml`
- `.github/workflows/ci.yml`
- `crates/fission-tauri/package.json`
