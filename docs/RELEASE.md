# Release checklist

**Last verified:** 2026-05-02

This checklist closes the loop between **tag-driven CD** ([`.github/workflows/cd.yml`](../.github/workflows/cd.yml)) and **documented expectations** for external evaluators ([`docs/EVALUATION.md`](EVALUATION.md)). A release is “ready” when the items below are satisfied for the tagged commit.

## 1. Version and changelog

- [ ] Version matches [`docs/VERSIONING.md`](VERSIONING.md) rules and the git tag (`v*.*.*`).
- [ ] User-visible changes recorded under [`docs/changelog/new/`](changelog/new/) (see **Changelog policy** below).
- [ ] Breaking or experimental CLI flags called out explicitly in changelog notes.

## 2. Build matrix (must match CD)

`cd.yml` currently builds **`fission-cli`** in **release** mode for:

| Platform | Rust target | Published asset prefix |
|----------|-------------|-------------------------|
| Linux x64 | `x86_64-unknown-linux-gnu` | `fission-linux-x64` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `fission-macos-arm64` |
| Windows x64 | `x86_64-pc-windows-msvc` | `fission-windows-x64` |

Local dry-run:

```bash
cargo build -p fission-cli --locked --release
```

Cross-target developers should mirror the matrix with `rustup target add …` as needed.

## 3. Smoke validation (evaluation path)

Run the **30-minute** (or shorter) path in [`docs/EVALUATION.md`](EVALUATION.md) on at least **one** representative Windows x64 binary from:

[`benchmark/binary/x86-64/window/small/`](../benchmark/binary/x86-64/window/small/)

Capture note-level anomalies (JSON shape shifts, crashers) in changelog **Known issues**.

## 4. Quality evidence (recommended)

Not strictly gated by `cd.yml`, but strongly recommended before tagging:

- [ ] `cargo test -p fission-pcode` (and crates touched by the release).
- [ ] `cargo run -p fission-automation -- nir-check --lane nir` with `--fail-on-stop` when automation semantics changed ([`crates/fission-automation/AGENTS.md`](../crates/fission-automation/AGENTS.md)).

Heavy CI uploads automation artifacts under `benchmark/artifacts/automation/` — attach links or excerpts to the GitHub Release discussion when helpful.

## 5. Benchmark corpus (optional attachment)

If the release claims corpus-wide improvements:

- Capture `benchmark_compact_summary.json` (see [`docs/QUALITY_METRICS.md`](QUALITY_METRICS.md)) from a reproducible run directory under `benchmark/artifacts/full_benchmark/`.
- Reference the exact runner invocation from [`benchmark/BENCHMARK_GUIDE.md`](../benchmark/BENCHMARK_GUIDE.md).

## 6. Experimental flags

Document any **experimental** CLI flags or environment variables that ship enabled-by-default or opt-in, with migration guidance.

## 7. Supply chain hygiene (SBOM)

Full SBOM generation is **not** yet automated in CI. Acceptable interim steps:

- Run a candidate tool locally (for example **`cargo-cyclonedx`**, **`cargo syft`**, or **`cargo sbom`** when adopted) against `-p fission-cli` and attach CycloneDX/SPDX to the Release assets when distributing binaries broadly.
- Track Rust/npm bumps via [`.github/dependabot.yml`](../.github/dependabot.yml).

## Changelog policy

Development entries live in **`docs/changelog/new/`** as dated Markdown files (`YYYYMMDD_Changelog.md`). At release time, maintainers consolidate into user-facing notes as the project prefers — but **every tag must reference** what moved in `docs/changelog/new/` for that cycle.

Contributors: see [`CONTRIBUTING.md`](../CONTRIBUTING.md) § Documentation for where to mention user-facing edits.
