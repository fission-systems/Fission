## Summary

<!-- What does this PR change and why? Link issues (e.g. `Closes #123`). -->

## Semantic ownership

<!-- Check every area you materially changed; reviewers use this for routing. -->

- [ ] **NIR / structuring / pcode** (`crates/fission-pcode/`, `AGENTS.md` “NIR structuring”)
- [ ] **Static facts / native prep** (`crates/fission-static/`)
- [ ] **Decompilation orchestration / Rust-Sleigh** (`crates/fission-decompiler/`)
- [ ] **Loader / binary parsing** (`crates/fission-loader/`)
- [ ] **CLI / UX** (`crates/fission-cli/`)
- [ ] **Desktop (Tauri)** (`crates/fission-tauri/`)
- [ ] **Automation / quality lanes** (`crates/fission-automation/`, including `crates/fission-automation/config/`)
- [ ] **Benchmark harness / corpus** (`benchmark/full_benchmark/`, `benchmark/config/`)
- [ ] **Docs / process only** (no semantic code paths)
- [ ] **CI / release plumbing** (`.github/`, `docs/RELEASE.md`)

## Telemetry and reporting

- [ ] Changes to decompiler semantics or inventory output: **confirmed** [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) and `fission-automation` aggregates stay aligned (or updated together with tests).
- [ ] No parallel ad-hoc telemetry JSON outside the established `summary.json` / benchmark contracts ([`docs/QUALITY_METRICS.md`](docs/QUALITY_METRICS.md)).

## ADR / design traceability

- [ ] Architectural trade-offs are covered by an existing [`docs/adr/`](docs/adr/) decision, **or** this PR adds/updates an ADR.
- ADR link(s): <!-- e.g. `docs/adr/0001-cli-first-external-surface.md` -->

## Testing

<!-- Commands you ran; paste key output if useful. -->

```bash

```
