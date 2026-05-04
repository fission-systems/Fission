# `crates/fission-signatures/data`

Canonical WinAPI/type/FID/signature **data files** live under [`utils/signatures/`](../../../utils/signatures/). This crate owns **loaders, parsers, providers, and matchers** only.

Do **not** add new imported/generated corpora under legacy subtrees:

- `win_api/`
- `win_types/`
- `signatures/`

Those directories are forbidden by CI (`scripts/check_signatures_canonical_data.sh`). Minimal **test-only** fixtures belong under `crates/fission-signatures/tests/` (or another documented fixture root), not here.
