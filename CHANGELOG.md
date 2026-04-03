# Changelog

## 2026-04-03

### Added
- Sleigh converter statement-level UserCall lowering to CALLOTHER.
- New Sleigh converter modules for export and user-call handling.
- Additional Sleigh language/spec assets for converter and lifter validation.
- Utility inventory reader script for benchmark support.

### Changed
- Extended LocalGoto relative branch handling for broader signed delta resolution.
- Improved NIR relative branch target resolution and related CFG behavior.
- Updated NIR structuring and normalization paths, including guarded-tail and linearization work.
- Updated CLI one-shot decompile path and rendering/common argument handling.
- Updated automation reporting and native decompiler integration plumbing.

### Fixed
- Restored multiple pcode/NIR tests after structural model drift in basic block fields.
- Improved converter expression/assignment handling robustness and edge-case behavior.
- Synced decompiler extraction paths and headers for native bridge changes.

### Validation
- Verified Sleigh converter crate tests are passing.
- Verified pcode and CLI build checks pass with current integration state.
