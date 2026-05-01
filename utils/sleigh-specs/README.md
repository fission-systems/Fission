# SLEIGH Specs Snapshot

This directory is the canonical checked-in SLEIGH spec data root for Fission.

- Primary path: `utils/sleigh-specs/languages/`
- Manifest: `utils/sleigh-specs/ghidra_language_manifest.json`
- Integrity manifest: `utils/sleigh-specs/MANIFEST.sha256.json`

The `fission-sleigh` crate resolves specs in this order:

1. `FISSION_SLEIGH_SPEC_DIR`
2. repo-relative `utils/sleigh-specs`
3. legacy crate-local `crates/fission-sleigh/specs`

The legacy crate-local path remains a temporary fallback during migration. New
tooling should use this directory.
