# ADR 0005: Ghidra stays reference — Rust owns shipped semantics

**Status:** Accepted (context amended by [ADR 0017](0017-ghidra-is-not-an-oracle.md))  
**Last verified:** 2026-11-23

## Context

Ghidra is an indispensable reference for sleigh semantics and regression comparison, but importing its architecture wholesale would obscure licensing boundaries and ownership of the Rust implementation. It is not a correctness oracle: its SLEIGH specs have been wrong, and Fission, lifting from them, agreed with the errors ([ADR 0017](0017-ghidra-is-not-an-oracle.md)).

## Decision

Local Ghidra checkouts are **reference-only**. Packaged Ghidra-derived **data** lives under [`utils/ghidra-data/`](https://github.com/fission-systems/Fission/tree/main/utils/ghidra-data) with explicit notices ([`THIRD_PARTY.md`](https://github.com/fission-systems/Fission/blob/main/THIRD_PARTY.md)).

Production semantics ship from **`fission-sleigh` + `fission-pcode`** contracts, validated—not duplicated—from Ghidra behavior.

## Consequences

- Algorithmic parity work cites upstream reference modules and implements invariant-driven Rust equivalents.
- The [resource packaging policy](../CI_RELEASE_GATES.md#resource-bundle-utils-assets) defines which runtime data ships in release bundles.
