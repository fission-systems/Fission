# Release checklist

Follow [CI and release gates](CI_RELEASE_GATES.md) for gate policy and
repository setup. The [release workflow](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-tag.yml)
checks the candidate, calls [release E2E](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-e2e.yml)
and [package verification](https://github.com/fission-systems/Fission/blob/main/.github/workflows/cd.yml), and publishes the
verified artifacts.

## 1. Prepare, review and merge the Release PR

Follow the [release process](CI_RELEASE_GATES.md#release-process):

```bash
gh workflow run "Prepare Release PR" --ref main -f version=vX.Y.Z
```

Choose the version following [Versioning](VERSIONING.md). The workflow
bumps the version, drafts `docs/changelog/vX.Y.Z.md` from the merged PRs and
opens `release: vX.Y.Z`. Review the notes, fill every `TODO(release)` block
(the benchmark table and, when no issue is labelled, the known
limitations), and merge it. Then confirm a Fast Gate main push run (L0) and
dispatch Heavy (L1) on the merge commit, and dispatch the tag:

```bash
gh workflow run "Release Tag (CI green)" --ref main -f ref=<merge-sha>
```

The tag defaults to the version the Release PR recorded. The workflow
refuses a commit that is not the Release PR's merge on top of its candidate,
and notes with an unfilled block. It creates the tag only after release E2E
(L2) and all package checks (L3) succeed. Retry a failed publication with the
same SHA. The workflow refuses to overwrite published releases.

## 2. Check the packages

| Platform | Rust target | Runner |
|---|---|---|
| Linux x64 | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` (glibc 2.35 floor) |
| Linux arm64 | `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `macos-latest` |
| macOS Intel | `x86_64-apple-darwin` | `macos-15-intel` |
| Windows x64 | `x86_64-pc-windows-msvc` | `windows-latest` |

The package workflow unpacks each archive outside the source tree and checks
resource discovery, `info`, `raw-pcode`, and `decomp`. A PR or merge group
that changes relevant packaging paths rehearses the same matrix and uploads
candidate artifacts to Actions. The release workflow publishes those exact
verified bytes from its own candidate run.

Local package smoke on Apple Silicon:

```bash
cargo build -p fission-cli --locked --release
scripts/release/package.sh binary aarch64-apple-darwin target/release/fission_cli dist
scripts/release/smoke_package.sh dist/fission-aarch64-apple-darwin.tar.gz \
  crates/fission-sleigh/testdata/a64_csel_select.so
```

For a manual spot check, use the Windows x64 fixture in the
[external evaluation guide](EVALUATION.md).

## 3. Verify the release attachments

- Five `fission-<target>.tar.gz` archives (`.zip` on Windows), each with
  `bin/fission_cli` (`fission_cli.exe` on Windows) and `share/fission/`.
- `fission-resources.tar.gz` for installs that need a separate resource bundle.
- `fission-cli.rb` for the
  [Homebrew tap](https://github.com/fission-systems/homebrew-tap).
- `fission_cli-x86_64-unknown-linux-gnu`, the standalone Linux x64 benchmark binary.
- A checksum file for each payload, the aggregate `SHA256SUMS`, and
  `release.json` identifying the candidate SHA and tag.

The publish job checks staged digests, generates GitHub build provenance,
creates or updates the draft, downloads the attachments and checks them before
publication.

## 4. Attach quality evidence when claiming an improvement

Link before/after full FreshBench M1 runs with the same corpus id and scorer
(`freshbench sweep diff` refuses runs scored by different scorers). Report
output changes, failure transitions, GED perfect count next to distance, and
type results. Follow the
[FreshBench evidence rules](BENCHMARK_FRESHBENCH.md).
