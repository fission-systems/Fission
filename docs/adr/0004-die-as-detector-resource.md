# ADR 0004: Detect It Easy as versioned detector corpus

**Status:** Accepted  
**Last verified:** 2026-05-02

## Context

Packers and toolchain fingerprints evolve continuously; bespoke string matching in Rust duplicates DiE maintenance upstream.

## Decision

Treat bundled Detect It Easy assets under [`utils/signatures/die/detect-it-easy/`](../../utils/signatures/die/detect-it-easy/) as **versioned corpus data** synchronized from upstream ([`THIRD_PARTY.md`](../../THIRD_PARTY.md)).

Rust integrations **consume** outputs/rules rather than forking unrelated logic inline.

## Consequences

- Detector gaps surface as structured unsupported outcomes suitable for metrics ([`docs/QUALITY_METRICS.md`](../QUALITY_METRICS.md)).
- Updating signatures is an operational task with license reminders (MIT upstream).
