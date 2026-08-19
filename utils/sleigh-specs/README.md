# SLEIGH Specs Snapshot

This directory is the canonical checked-in SLEIGH spec data root for Fission.

- Primary path: `utils/sleigh-specs/languages/`
- Compiled lift artifacts: `utils/sleigh-specs/compiled/` (Ghidra 12.0.4 `.sla` snapshots)
- Manifest: `utils/sleigh-specs/ghidra_language_manifest.json`
- Integrity manifest: `utils/sleigh-specs/MANIFEST.sha256.json`

The `fission-sleigh` crate resolves specs in this order:

1. `FISSION_SLEIGH_SPEC_DIR`
2. repo-relative `utils/sleigh-specs`
3. legacy crate-local `crates/fission-sleigh/specs`

Production lift uses checked-in `.slaspec` sources plus the matching compiled `.sla`
ConstructTpl overlay from `compiled/<arch>/<entry>.sla`. Executable runtime frontends
require the overlay; slaspec-only lift is not supported for production paths. Vendor
Ghidra installs are not required at runtime.

The legacy crate-local path remains a temporary fallback during migration. New
tooling should use this directory.
