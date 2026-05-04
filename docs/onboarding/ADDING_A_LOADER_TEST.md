# Adding a loader test

**Last verified:** 2026-05-02

Loader tests usually live next to the module they protect as `#[cfg(test)] mod tests { ... }` under [`crates/fission-loader/src/`](../../crates/fission-loader/src/).

## Pick the right seam

| Change area | Likely file |
|-------------|-------------|
| PE / ELF / Mach-O parsing | `loader/pe/`, `loader/formats/*` |
| Detection / DiE integration | `detector/` ([`docs/adr/0004-die-as-detector-resource.md`](../adr/0004-die-as-detector-resource.md)) |
| Pipeline wiring | `loader/pipeline.rs`, `loader/mod.rs` |

## Write a focused unit test

1. Create or extend a `tests` module in the touched file (see existing examples: `loader/mod.rs`, `detector/mod.rs`).
2. Prefer **inline byte fixtures** or tiny files from `benchmark/binary/` already redistributable.
3. Assert **public error types / invariants**, not incidental log text.

Run:

```bash
cargo test -p fission-loader
```

## Integration expectations

If your change affects CLI-visible `info`/`list`, mention it in the PR description and upcoming **version-scoped** release notes (GitHub Release / `CHANGELOG.md` when present); archived dated logs live under [`docs/changelog/Legacy/`](../changelog/Legacy/).

## Security reminder

Do not commit malware; follow [`docs/MALWARE_SAMPLE_POLICY.md`](../MALWARE_SAMPLE_POLICY.md).
