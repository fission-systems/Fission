# ADR 0005: Ghidra stays reference — Rust owns shipped semantics

**Status:** Accepted  
**Last verified:** 2026-05-02

## Context

Ghidra is an indispensable oracle for sleigh semantics and regression comparison, but importing its architecture wholesale conflicts with AGPL licensing clarity and Rust-first ownership goals.

## Decision

Vendor Ghidra trees remain **reference-only** ([`vendor/MANIFEST.md`](../../vendor/MANIFEST.md)); packaged Ghidra-derived **data** lives under [`utils/ghidra-data/`](../../utils/ghidra-data/) with explicit notices ([`THIRD_PARTY.md`](../../THIRD_PARTY.md)).

Production semantics ship from **`fission-sleigh` + `fission-pcode`** contracts, validated—not duplicated—from Ghidra behavior.

## Consequences

- Algorithmic parity work cites Ghidra modules but implements invariant-driven Rust equivalents ([`vendor/ghidra/`](../../vendor/ghidra/), [`vendor/retdec-5.0/`](../../vendor/retdec-5.0/) comparison allowed).
- Release bundles may optionally include helper resources (`cd.yml` hook); absence must remain documented.
