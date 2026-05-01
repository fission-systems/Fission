# ADR 0001: CLI-first external surface

**Status:** Accepted  
**Last verified:** 2026-05-02

## Context

External teams evaluate Fission headlessly and need stable invocation and observable artifacts. A prematurely frozen Rust crate API spreads semver liability across unrelated internals.

## Decision

Treat **`fission_cli`** as the **primary supported external product surface** while library crates remain workspace internals unless explicitly documented otherwise ([`docs/EVALUATION.md`](../EVALUATION.md)).

## Consequences

- Contract tests and release gates emphasize CLI behavior and JSON outputs.
- Crate-level semver for unpublished internals is secondary to CLI semver ([`docs/VERSIONING.md`](../VERSIONING.md)).
- Desktop (`fission-tauri`) builds on the same pipeline assumptions but does not redefine semantic contracts.
