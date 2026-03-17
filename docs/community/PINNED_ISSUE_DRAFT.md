# Fission Public Roadmap / How To Contribute

Fission is now public, but it is still an actively changing reverse-engineering and decompilation workspace.

## Current State

- `legacy` is still the stable default decompilation path.
- `mlil-preview` is the forward architecture path built around Rust-owned NIR/HIR, normalization, structuring, and printing.
- The repository is usable for exploration and development, but it is not yet a polished end-user product.

## Where Contributions Are Most Helpful

- Preview decompiler quality on real-world functions
- Structured control-flow recovery
- Type and symbol surfacing
- Benchmark automation and regression tracking
- Desktop workflow polish in the Tauri frontend
- Documentation cleanup and English-first public docs

## Before Opening A PR

- Read [`CONTRIBUTING.md`](../../CONTRIBUTING.md)
- Read [`CLA.md`](../../CLA.md)
- Check the architecture notes in [`docs/architecture/ARCHITECTURE.md`](../architecture/ARCHITECTURE.md)
- Prefer focused patches over broad refactors

## Contribution Expectations

- Keep changes small and reviewable
- Add or update tests when behavior changes
- Preserve current engine/fallback policy unless the change is explicitly about that policy
- Avoid mixing unrelated cleanup with behavior changes

## Good First Areas

- Documentation fixes
- README cleanup
- Benchmark/report ergonomics
- Small preview normalization or printer improvements
- Tauri UI polish that does not change core engine semantics

## Discussion

If you want to contribute to a larger subsystem, open a discussion or issue first so the work aligns with the current architecture direction.
