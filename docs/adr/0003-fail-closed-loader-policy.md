# ADR 0003: Fail-closed loader for unsupported or ambiguous binaries

**Status:** Accepted  
**Last verified:** 2026-05-02

## Context

Silent best-effort decoding invites misleading decompilation artifacts—especially across PE variants, packed blobs, and incomplete opinions.

## Decision

When the loader cannot establish **minimal trustworthy facts** needed by downstream lifting (regions, entry semantics, integrity checks configured by policy), **fail closed** with explicit errors or guarded fallback modes rather than emitting speculative IR.

## Consequences

- UX surfaces error strings/diagnostics operators can action (`fission_cli` inventory paths).
- Benchmark automation may record loader failures distinctly from NIR failures ([`docs/QUALITY_METRICS.md`](../QUALITY_METRICS.md)).
- Tests should encode negative cases (`crates/fission-loader/`).
