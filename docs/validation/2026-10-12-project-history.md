# RE project history validation — 2026-10-12

## Scope and baseline

First milestone of the headless RE-tool direction: persist and reverse analyst
choices across invocations. Baseline: `59e250532c8cc768c376c7c0a8d04e512e9bc947`
(`main`), with an isolated checkout and checkout-local Cargo target. Existing
dirty decompiler changes and PR #176 were preserved.

The existing project already persisted names, signatures, notes and debugger
stops. It had no revision journal, used direct file truncation on save, and
printed `db` success before that save completed. This change extends
`fission-project`, the mutable analyst-decision owner. It does not change the
immutable program metadata owner or decompiler algorithms.

## Results

| Gate | Result |
| --- | --- |
| Focused history/storage/analysis-db CLI nextest | 23 passed; 121 filtered out |
| Entire `fission-project` + `fission-cli` nextest | 144 passed; 0 skipped |
| `cargo check -p fission-project -p fission-cli` | Passed |
| Affected-crate clippy with CI warning policy | Passed |
| Locked release CLI build | Passed |
| Workspace rustfmt check and diff whitespace check | Passed |
| Separate-process release CLI smoke | 43 commands passed + 2 binary-hash checks |

Focused runs reported nextest leak warnings (two on the initial run, one on a
repeat); both full-suite runs finished without those warnings. This is recorded
rather than treating the focused run as evidence of process cleanup.

The release smoke used the checked-in PE `win_x64_write.exe` and ELF
`x64_static_printf_malloc.elf` fixtures. Each was copied into a private artifact
directory. For **each binary**, independent commands named a function, saved a
signature, note and breakpoint, compared a saved snapshot, undid four revisions,
and redid them after loading the project again. The restored history matched;
`list` saw the restored name; SHA-256 of the binary bytes was unchanged.

The PE smoke also ran `info`, `disasm`, `decomp --addr`, typed program metadata
inventory and deprecated flat `--info` (including the warning). Resource status
was exercised in text and JSON. New `db --json` paths parsed as one JSON stdout
document. Existing crate tests additionally exercised signatures in callers,
restored debugger stops and interactive emulator sessions.

New regression cases cover schema 1 migration, all five typed decision kinds,
no-op redo preservation, redo branch replacement without ID reuse, removals,
legacy stop ordering, malformed history, cross-binary comparison, transaction
conflicts without partial undo, writer exclusion, failed replacement cleanup,
symlink save refusal, permission retention, and no success stdout on failed save.

## Reproduction and artifacts

```bash
cargo nextest run -p fission-project -p fission-cli \
  -E 'test(history) | test(storage) | binary(analysis_db_cli)'
cargo nextest run -p fission-project -p fission-cli --status-level all
cargo check -p fission-project -p fission-cli
cargo clippy -p fission-project -p fission-cli --locked -- \
  -D warnings -A clippy::style -A clippy::complexity
cargo build -p fission-cli --release --locked
cargo fmt --all --check
```

Local logs, a source/binary hash manifest, the smoke script and all command
stdout/stderr are in `benchmark/artifacts/re-workspace/2026-10-12-project-history/`
in the primary checkout. These local artifacts are ignored by Git; regression
cases and this report are committed. Run the artifact smoke script with
`--root <checkout> --cli <release-cli> --out <new-empty-output-directory>`.

## Limits

- Local platform: macOS arm64. Linux is covered by PR CI; local Windows behavior
  and power-loss recovery have not been validated here.
- Cooperating writers are excluded by a lock file. External editors are not
  coordinated; a crash can leave a lock requiring manual recovery after the
  writer exits. Atomic replacement is not a complete power-loss guarantee.
- Journal entries are created by `db` or explicit library `record_edit` calls;
  low-level setters and legacy `AnalysisProject` consumers are not silently
  converted into journal transactions. Watchpoint changes are covered by the
  library; no new watchpoint CLI edit command is added.
- History reverses analyst decisions, not binary patches or derived analysis.
  Schema 2 is readable by this build; older schema 1-only releases refuse it.
- These are product workflow results. No DecBench ranking, decompiler quality,
  or full RE-tool/1.0 readiness improvement is asserted.
