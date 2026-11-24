# Versioning policy

**Last verified:** 2026-05-02

## Semantic versioning

Fission follows **SemVer** (`MAJOR.MINOR.PATCH`), including during **`0.y.z`**:

- **`MAJOR`** (including `0` → `1`): Breaking changes to documented CLI contracts, artifact JSON schemas promised as stable in [`docs/EVALUATION.md`](EVALUATION.md), or installation/layout guarantees described in release notes.
- **`MINOR`**: Backward-compatible CLI additions, new optional flags, additive JSON fields (clients must tolerate unknown keys), performance improvements without contract breaks.
- **`PATCH`**: Bug fixes and internal refactors that do not change outward contracts.

While in `0.x`, **minor releases may still introduce significant behavior changes** if they are flagged in release notes and evaluation docs; patch releases should remain low-risk.

## Tags and GitHub Releases

- Release tags match **`vMAJOR.MINOR.PATCH`** (for example `v0.4.2`).
- **Do not auto-tag every `main` push.** Tags are intentional promotions after
  layered gates (Fast Gate + Heavy + Release E2E). See
  [`docs/CI_RELEASE_GATES.md`](CI_RELEASE_GATES.md).
- Preferred path: Actions → **Release Tag (CI green)**
  ([`release-tag.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-tag.yml)).
- Continuous delivery builds from tag pushes per [`.github/workflows/cd.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/cd.yml).

## Branches

- **`main`** is the integration branch; tags are cut from commits that pass merge gates described in [`CONTRIBUTING.md`](https://github.com/fission-systems/Fission/blob/main/CONTRIBUTING.md).
- Long-lived release branches are **optional**; if adopted later, document them here and in [`docs/RELEASE.md`](RELEASE.md).

## Pre-releases

If you need `-alpha` / `-beta` suffixes, prefer **GitHub prerelease** toggles plus clear notes; keep tag names consistent with tooling expectations in `cd.yml`.

## Tag, binary and bundle identity

The tag without its `v` must equal the `[package]` version of
`crates/fission-cli/Cargo.toml`, prerelease suffix included: `v0.5.0-rc.1`
needs version `0.5.0-rc.1`. Other workspace crates are versioned in lockstep
but are not checked. A release build embeds the tag and commit
(`fission_cli --version`, `resources status --json`), and every packaged
resource bundle carries `fission-resources.json` naming the same release.
`release-tag.yml` refuses to tag or publish when the three disagree
(`scripts/release/check_identity.py`).
