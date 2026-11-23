# CI and CD workflow conventions

See [CI and release gates](../docs/CI_RELEASE_GATES.md) for required evidence and
the [operator guide](CI_CD_GUIDE.md) for running and diagnosing the workflows.

- Select work from event base and head SHAs. Keep `Detect changes` and
  `CI Summary` stable for branch protection. Selected lanes must complete
  successfully. Keep nextest's zero-test failure enabled.
- Place path predicates in the [change classifier](../scripts/ci/change_plan.py).
  Test changes with the [classifier suite](../scripts/ci/test_change_plan.py)
  and run the changed workflow or lane on a PR.
- Reuse the existing
  [Rust setup](workflows/reusable-setup-rust.yml),
  [test](workflows/reusable-run-tests.yml), and
  [CLI smoke](workflows/reusable-cli-smoke.yml) workflows. Use nextest for Rust
  test suites. Use Cargo's test harness for doctests.
- Pin third-party actions by full commit SHA, declare minimal workflow and
  job permissions, and pass event-controlled values to shell steps through
  environment variables. Run `actionlint` and `zizmor` for workflow changes.
- Verify release candidates from the exact SHA used for L0 and L1. Pass the
  staged archives from [the package workflow](workflows/cd.yml) to
  [the publisher](workflows/release-tag.yml) by artifact instead of rebuilding.
