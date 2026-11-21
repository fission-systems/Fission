# Fission CLI Guide

Generated: 2026-04-21  
Scope: `crates/fission-cli/`

## Overview

`fission-cli` is the human-facing one-shot surface plus operator inventory entrypoint. It owns command parsing, UX grouping, compatibility shims, and output routing. It does not own semantic decompiler repair.

## Current command model

Canonical subcommands (the source of truth is `CANONICAL_SUBCOMMANDS` in
`src/cli/args/mod.rs`; `fission_cli --help` prints them):

| Group | Commands | Owner of the semantics |
|---|---|---|
| Static views | `info`, `list`, `disasm`, `strings`, `hex`, `search`, `xrefs`, `callgraph`, `whatis`, `types` | `fission-loader`, `fission-analysis-db`, `fission-static` |
| Decompilation | `decomp` | `fission-decompiler` |
| IR diagnostics | `raw-pcode`, `pcode-stages`, `nir-stats`, `pcode-topology` | `fission-sleigh`, `fission-pcode` |
| Signatures | `identify`, `similar`, `api` | `fission-decompiler::fid`, `fission-signatures` |
| Changes and decisions | `patch`, `db` | CLI (`patch`), `fission-project` (`db`) |
| Batch | `inventory {program-metadata, function-facts, preview-candidates}` | `fission-static`, `fission-analysis-db` |
| Dynamic | `sandbox`, `call`, `debug` (+ `debug session`), `verify` | `fission-emulator`, `fission-dynamic`, `fission-dir` |
| Scripting | `script {check, run}` | `fission-script` |
| Environment and contract | `resources status`, `languages`, `schema`, `skill` | `fission-core`, `fission-sleigh` |

Global flags: `--resource-root`, `--signature-path`, `--language`, `--base`,
`--entry`, `--arch`, `--db`, `--no-db`, `--contract`, `--fail-on-partial`.
A global flag a command cannot honor must warn (`ignored_option_warnings`,
`raw_load_warnings`), never be dropped silently; so must a per-command flag the
command parses and does not read (`--compiler-id`, `--profile`, `--format`,
`--engine`).

Legacy flat invocations are still supported as deprecated compatibility shims. They must normalize into the same internal execution path instead of creating a second behavior path.

**Resource bundle:** the global **`--resource-root`** flag is intentional. It must only prime `fission_core::resource_roots::set_cli_resource_bundle_root` during canonical parse (see `args.rs`), not duplicate path logic in CLI. Prefer `docs/CLI.md` § *Runtime resource bundle* for operator-facing documentation.

## Ownership boundaries

| Area | Location | Notes |
|---|---|---|
| Canonical parser + legacy shim | `src/cli/args.rs` | Normalize to internal `OneShotArgs`; apply `set_cli_resource_bundle_root` after `CliArgs` parse |
| One-shot dispatch | `src/cli/oneshot/mod.rs` | Subcommand-driven dispatch only; `resources status` has no binary |
| Resources diagnostics | `src/cli/resources.rs` | `resources status` output only; semantics stay in `fission-core` `PATHS` |
| Decomp execution | `src/cli/oneshot/decompile*/` | Output semantics should remain stable across parser refactors |
| Inventory/operator emitters | `src/cli/oneshot/inventory/` | Keep batch-only knobs off the primary human-facing path |
| Output helpers | `src/cli/output/` | Rendering/serialization only, not semantic repair |

## Rules

1. Keep canonical subcommands as the source of truth for human-facing CLI shape.
2. Keep legacy flat syntax as a translation layer only; never maintain a second execution implementation.
3. Move new flags to the subcommand that owns the behavior instead of growing one global option surface — **except** `--resource-root`, which is a deliberate process-wide bundle override wired to `fission-core`.
4. Keep inventory/batch/operator controls under `inventory`, not mixed into `decomp`.
5. Preserve JSON, benchmark, and inventory payload compatibility unless the change explicitly targets those schemas.

## Anti-patterns

- Do not add semantic repair logic in CLI parsing or output code.
- Do not add new inventory-only flags to `decomp`.
- Do not let legacy shims drift from canonical subcommand behavior.
- Do not rewrite output payloads as part of a surface-only refactor.

## Validation

```bash
cargo test -p fission-cli
cargo check -p fission-cli
cargo build -p fission-cli
```

Manual validation should cover at least:

- one `info` command
- one `list` command
- one `disasm` command
- one `decomp --addr` command
- one `inventory` command
- `resources status` (text and `--json`)
- one legacy flat command that emits a deprecation warning
