## Summary

<!-- What does this PR change and why? Link issues (e.g. `Closes #123`). -->

## Change Type

- [ ] Decompiler semantics / IR / structuring
- [ ] Loader / binary format support
- [ ] Static facts / native preparation
- [ ] Decompilation orchestration / Rust-Sleigh
- [ ] CLI / product surface
- [ ] Desktop UI / Dioxus
- [ ] Benchmark / automation
- [ ] Documentation only
- [ ] Build / CI / release infrastructure

## Semantic Ownership

<!-- Check every area you materially changed; reviewers use this for routing. -->

- [ ] **NIR / structuring / pcode** (`crates/fission-pcode/`, `AGENTS.md` “NIR structuring”)
- [ ] **Static facts / native prep** (`crates/fission-static/`)
- [ ] **Decompilation orchestration / Rust-Sleigh** (`crates/fission-decompiler/`)
- [ ] **Loader / binary parsing** (`crates/fission-loader/`)
- [ ] **CLI / UX** (`crates/fission-cli/`)
- [ ] **Desktop (Dioxus)** (`crates/fission-dioxus/`)
- [ ] **Automation / quality lanes** (`crates/fission-automation/`, including `crates/fission-automation/config/`)
- [ ] **Benchmark harness / corpus** (`benchmark/source_semantic_benchmark/`, `benchmark/full_benchmark/`, `benchmark/config/`)
- [ ] **Docs / process only** (no semantic code paths)
- [ ] **CI / release plumbing** (`.github/`, `docs/RELEASE.md`)

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets`
- [ ] `cargo test --all`
- [ ] Source semantic smoke
- [ ] Feature-shape canaries
- [ ] Full/source-owned benchmark lane, if relevant
- [ ] Not run — reason:

<details>
<summary>Commands / key output</summary>

```bash

```

</details>

## Benchmark Impact

<!-- Paste paths or key metric deltas when relevant. Use `n/a` for docs-only changes. -->

```text
source_semantic_summary: <path or n/a>
weighted_semantic_similarity: <before> -> <after>
behavior_pass_rate: <before> -> <after>
comparison_outcome: <improved|regressed|mixed|unchanged|n/a>
```

## Telemetry and Reporting

- [ ] Changes to decompiler semantics or inventory output: **confirmed** [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) and `fission-automation` aggregates stay aligned, or this PR updates them together with tests.
- [ ] No parallel ad-hoc telemetry JSON outside the established `summary.json` / benchmark contracts ([`docs/QUALITY_METRICS.md`](docs/QUALITY_METRICS.md)).

## ADR / Design Traceability

- [ ] Architectural trade-offs are covered by an existing [`docs/adr/`](docs/adr/) decision, **or** this PR adds/updates an ADR.
- ADR link(s): <!-- e.g. `docs/adr/0001-cli-first-external-surface.md` -->

## Risk Notes

> [!IMPORTANT]
> If this changes semantic recovery, structuring, loader classification, or benchmark gates, include the exact reproduction command and artifact path.

## Checklist

- [ ] Documentation updated or not needed
- [ ] New/changed behavior is covered by tests, benchmark rows, or an explicit reason
- [ ] Generated artifacts, local corpora, and downloaded samples are not committed
