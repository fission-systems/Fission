# Release checklist

**Last verified:** 2026-07-15

This checklist closes the loop between **tag-driven CD** ([`.github/workflows/cd.yml`](../.github/workflows/cd.yml)) and **documented expectations** for external evaluators ([`docs/EVALUATION.md`](EVALUATION.md)). A release is “ready” when the items below are satisfied for the tagged commit.

**Gate policy (L0–L3):** [`docs/CI_RELEASE_GATES.md`](CI_RELEASE_GATES.md).

## Minimum release gate

A tagged release is eligible when:

- **L0** [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) has a successful **push** run on the tagged commit.
- **L1** [`.github/workflows/ci-heavy.yml`](../.github/workflows/ci-heavy.yml) has a successful run on that same commit (main push, nightly, or dispatch).
- **L2** [`.github/workflows/release-e2e.yml`](../.github/workflows/release-e2e.yml) succeeds on that commit (enforced by [release-tag.yml](../.github/workflows/release-tag.yml) before the tag is created).
- Prefer creating tags only via Actions → **Release Tag (CI green)** so the above are machine-checked and the tag points at the verified SHA.
- [`cd.yml`](../.github/workflows/cd.yml) publishes all expected **`fission-cli`** assets:
  - `fission-linux-x64`
  - `fission-macos-arm64`
  - `fission-windows-x64`
- The **30-minute** evaluation path in [`docs/EVALUATION.md`](EVALUATION.md) passes on at least **one** Windows x64 sample binary.
- If corpus-wide quality is claimed, attach or link `benchmark_compact_summary.json` and confirm:
  - `release_promotion_allowed` is `true`, **or**
  - any `promotion_blockers` are explicitly documented in release notes.

The numbered sections below expand this gate into operational detail.

## 1. Version and changelog

- [ ] Version matches [`docs/VERSIONING.md`](VERSIONING.md) rules and the git tag (`v*.*.*`).
- [ ] User-visible changes summarized for the tag (GitHub Release notes and/or repo `CHANGELOG.md` when adopted); deep-dated logs prior to version-scoped notes live under [`docs/changelog/Legacy/`](changelog/Legacy/).
- [ ] Breaking or experimental CLI flags called out explicitly in release notes.

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

Capture note-level anomalies (JSON shape shifts, crashers) in release notes **Known issues**.

## 4. Quality evidence (recommended)

Not strictly gated by `cd.yml`, but strongly recommended before tagging:

- [ ] `cargo nextest run -p fission-pcode` (and crates touched by the release; use `cargo test` only when doctests or harness-specific behavior must be checked).
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

**Current:** Prefer **version-scoped** notes tied to SemVer tags / GitHub Releases (and a root `CHANGELOG.md` when the project adopts one).

**Archive:** Earlier date-stamped engineering logs (`YYYYMMDD_Changelog.md`) were consolidated under [`docs/changelog/Legacy/`](changelog/Legacy/). [`docs/changelog/new/README.md`](changelog/new/README.md) explains the retired scratch folder.

Contributors: see [`CONTRIBUTING.md`](../CONTRIBUTING.md) § Documentation for where to mention user-facing edits.
