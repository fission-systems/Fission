# Heavy work off the development machine

A full test suite, a FreshBench sweep or a bisect over many commits loads one
laptop for an hour and fills its disk with `target/` directories. Each one
has a place to run that is not the laptop. The repository is public, so
GitHub-hosted runners cost nothing. They share the organization's
concurrency limit of 60 jobs, so start a large fan-out when the queue is
quiet.

| Work | Where | How |
|---|---|---|
| A test suite, `check` or `clippy` on your working tree | hosted runner | `cargo xtask remote nextest run -p <crate>` |
| The same on Windows or macOS | hosted runner | `cargo xtask remote --os windows -- check -p fission-cli` |
| Your commits against CI's Fast Gate | local, fast | `cargo xtask preflight` |
| What a decompiler change does to output and speed | the PR's base-vs-head check | open the PR; the base side comes from the output ledger |
| Which main commit changed one function's output | ledger artifacts, no build | `python3 scripts/ci/output_ledger.py history <key>` |
| Which commit broke a nightly | parallel bisect | automatic on a nightly regression, or `gh workflow run parallel-bisect.yml -f good=<sha> -f bad=<sha> -f probe=output -f build=gcc-O2` |
| A full FreshBench M1 sweep | nightly sweep | `gh workflow run freshbench-sweep.yml` |
| A long agent task that only needs the repository | Claude Code cloud session | start it from a pushed branch; see below |

What stays local is what needs a fast loop: `cargo build --profile
quick-release`, a targeted `cargo nextest run -p <crate> <filter>`, and
reading decompiler output on one binary.

## `cargo xtask remote`

It snapshots the working tree, including uncommitted and untracked files, as
a commit on a temporary `remote/<sha>` branch. Then it:
- dispatches `remote.yml` there and waits for the run;
- prints the log and exits with the remote command's status;
- deletes the branch unless `--keep` is given.

Arguments are passed to `cargo` without a shell. `--artifacts <path>` brings
files back under `target/remote/<run id>/`. No other workflow runs on a
`remote/*` push.

## Cloud sessions

A Claude Code cloud session runs from the GitHub repository, not this
machine. It starts from a pushed branch, or from `main` when the session
does not run in a worktree, so commit and push first. It cannot see anything
that is only on the laptop:
- `~/fission-benchmark`;
- local FreshBench data;
- unpushed branches;
- gitignored files.

Give it work whose inputs are in the repository, or fetched by the CI scripts
it can run. Measurements that need the local corpus stay with the CI
workflows above.

Cloud sessions and hosted runners follow the same measurement rules as local
runs ([CI and release gates](../CI_RELEASE_GATES.md),
[FreshBench](../BENCHMARK_FRESHBENCH.md)):
- telemetry off;
- corpus binaries are decompiled, never executed;
- no analyzed binary, decompiled output or credential is uploaded anywhere
  except to this repository's own workflow artifacts.
