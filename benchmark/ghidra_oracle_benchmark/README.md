# Ghidra oracle export benchmark

PyGhidra + Ghidra headless dumps **oracle facts beyond decompiled C text** (xrefs, call targets, string references, signature / parameter summaries, decompile success or failure reasons, etc.) to JSON driven by a manifest.  
This lane is separate from Rust crate unit tests; fixture paths belong **only in manifests**.

## Requirements

- Ghidra install directory (`GHIDRA_INSTALL_DIR` or `--ghidra-dir`)
- Python package `pyghidra` (`pip install pyghidra`)
- For file-backed manifests, the binary must exist locally (e.g. an `.exe` built with MinGW in CI)

## Manifest schema

Top level:

- `binaries` (array): each element is one load unit.

Binary entry fields:

| Field | Meaning |
|------|---------|
| `id` | Stable string ID |
| `path` | Repo-relative path (required for file loads) |
| `hex_bytes` | Hex string (optional whitespace); used instead of `path` for synthetic loads |
| `program_name` | Program name for synthetic loads (default `synthetic`) |
| `language` | Ghidra language ID for synthetic loads (default `DATA:LE:64:default`) |
| `loader` | Loader name (default `BinaryLoader`) |
| `rows` | Array of per-function targets (`addr`, optional `name`, optional `feature_group` / `feature`). If empty, emits a single binary snapshot row. |

Row entry fields:

- `addr`: Seed address (`0x...`)
- `name`: Optional name hint for seed resolution (same rules as [`grand_finale_support/runners.py`](../../full_benchmark/grand_finale_support/runners.py))

## Output schema

- `_meta`: tool name, manifest path / sha256, Ghidra path, wall-clock time, row count
- `rows[]`: each row includes `binary_snapshot` (binary-level summary), `ghidra` (function oracle payload), and seed matching metadata

Some fields may be absent depending on Ghidra version / API; reasons are recorded under `collector_warnings`.

## Examples

```bash
export GHIDRA_INSTALL_DIR=/path/to/ghidra_12.0.4_PUBLIC
python3 benchmark/ghidra_oracle_benchmark/export_oracle.py \
  --manifest benchmark/ghidra_oracle_benchmark/examples/smoke_manifest.json \
  --ghidra-dir "$GHIDRA_INSTALL_DIR" \
  --out benchmark/artifacts/ghidra_oracle/export_smoke.json \
  --per-function-timeout-sec 180
```

Synthetic bytes only (temporary projects live under `benchmark/artifacts/ghidra_oracle_micro/`, gitignored):

```bash
python3 benchmark/ghidra_oracle_benchmark/export_oracle.py \
  --manifest benchmark/ghidra_oracle_benchmark/examples/micro_manifest.json \
  --ghidra-dir "$GHIDRA_INSTALL_DIR" \
  --out benchmark/artifacts/ghidra_oracle/export_micro.json
```

## Notes

- File inputs use the legacy `pyghidra.open_program()` path, matching Grand Finale.
- Synthetic `hex_bytes` inputs use the newer `program_loader()` flow.
- Large `call_targets` / string lists are capped ([`collectors.py`](collectors.py)).
