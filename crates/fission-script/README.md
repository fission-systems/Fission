# fission-script

Embedded [**Rhai**](https://rhai.rs/) scripting over **read-only** loaded-binary inventory (`fission-loader`). Scripts receive a `binary` object (path, format, image base, functions, imports, exports, sections, strings) and call `emit(map)` to append structured findings. Evaluation uses sandbox-style Rhai limits (operations, string size, expression depth) plus wall-clock and finding/output caps.

## Phase scope

- **In:** crate API (`check_script`, `run_script`), `ScriptLimits`, JSON result schema (`ScriptRunResult`), CLI `fission_cli script check|run`, Balanced function discovery via `prepare_binary_for_script`.
- **Deferred:** PyO3 / bundled Python, `fission-python-bridge`, decomp debug bundles, NIR/HIR in scripts, filesystem/network/process permissions, TOML capability manifests (design hooks reserved).

## CLI

```bash
fission_cli script check --script examples/list_imports.rhai
fission_cli script run /path/to/binary --script examples/list_imports.rhai --json
```

## Security model (P0)

Deny-by-default host surface: no file, process, or environment bindings are registered on the Rhai engine beyond `binary` and `emit`.
