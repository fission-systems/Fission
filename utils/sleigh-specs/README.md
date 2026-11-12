# SLEIGH Specs Snapshot

This directory is the canonical checked-in SLEIGH spec data root for Fission.

- Primary path: `utils/sleigh-specs/languages/`
- Compiled lift artifacts: `utils/sleigh-specs/compiled/` (Ghidra 12.2 `.sla` snapshots, taken from the `ghidra_12.2_PUBLIC_20261016` release)
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

## Local deviations from upstream Ghidra 12.2

`languages/` is upstream `Ghidra/Processors/<P>/data/languages/` verbatim (the `.sla`
files go to `compiled/`), except for the following. Re-apply them when upgrading;
each is recorded in `MANIFEST.sha256.json` (`source_path`).

- `ARM/{ARMt,ARMtTHUMB,ARMt_v6,ARMt_v45,ARMCortex,ARM_v45}.pspec`: `ISAModeSwitch` and
  `TB` are marked `hidden="true"`, so decoder-only interworking state never surfaces as
  decompiled assignments.
- `x86/x86-64-swift.cspec` and its `<compiler id="swift">` entry in `x86/x86.ldefs`:
  Ghidra 12.2 removed them (Swift moved to cspec extensions under
  `Ghidra/Features/Swift/data/extensions`). They are kept from 12.0.4 so Swift x86-64
  binaries still select a Swift calling-convention spec. Not yet migrated to the extension
  model.
- `AARCH64/AARCH64neon.sinc` (and the three compiled `AARCH64*.sla`): upstream lifts every
  FCVT{A,M,N,P,Z}{S,U} rounding mode to a bare `trunc`, so `fcvtms`/`fcvtps`/`fcvtns`/`fcvtas`
  behave like `fcvtzs`. Each mode now has its own display table and constructors:
  `trunc(floor(x))` for M, `trunc(ceil(x))` for P, `trunc(round(x))` for N and A, `trunc(x)`
  for Z. N and A are approximations (SLEIGH has only half-up `round`, no ties-to-even or
  ties-away), and saturation is unchanged. Re-apply with
  `scripts/sleigh/apply_aarch64_fcvt_rounding.py` (needs a `sleigh` compiler of the same
  Ghidra release), then recompile the three `.sla` and refresh `MANIFEST.sha256.json`.
- `Toy/` is from the Ghidra source tree (it has no `data/languages` in the release) and
  has no compiled `.sla`.

Regenerate `ghidra_language_manifest.json` with
`cargo run --release -p fission-sleigh --example generate_sleigh_frontends`.
