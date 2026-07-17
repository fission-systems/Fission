# Debugging a decompilation failure

**Last verified:** 2026-05-02

Use this linear checklist before guessing at printer bugs.

## 1. Loader / facts (`fission-loader`)

Symptoms: CLI errors before listing functions, bogus sections, impossible addresses.

- Run `fission_cli info` + `list` on the smallest failing fixture ([`docs/EVALUATION.md`](../EVALUATION.md)).
- Fail-closed behavior is intentional ([`docs/adr/0003-fail-closed-loader-policy.md`](../adr/0003-fail-closed-loader-policy.md)).

## 2. Disassembly sanity (`fission_cli disasm`)

Symptoms: wrong opcode stream at entry.

- Confirm function boundaries from `list` match bytes under `disasm`.
- If boundaries lie, fix loader/symbol pipeline—not NIR first.

## 3. Lift / pcode (`fission-sleigh`)

Symptoms: absurd pcode volume or decode aborts.

- Compare against sleigh references under [`vendor/ghidra/`](../../vendor/ghidra/) conceptually—do **not** patch vendor copies for production fixes ([`docs/adr/0005-ghidra-reference-boundary.md`](../adr/0005-ghidra-reference-boundary.md)).

## 4. NIR build (`fission-pcode`)

Symptoms: empty bodies, chaotic control flow, telemetry explosions.

- Inspect embedded counters via automation summaries (`nir_build_stats_totals`, [`docs/QUALITY_METRICS.md`](../QUALITY_METRICS.md)).
- Canonical counters live in [`crates/fission-pcode/src/nir/ir/build_stats.rs`](../../crates/fission-pcode/src/nir/ir/build_stats.rs).

## 5. Structuring / normalization

Symptoms: duplicated labels, collapsed loops, odd regions.

- Fix in [`crates/fission-pcode/src/nir/structuring/`](../../crates/fission-pcode/src/nir/structuring/) per child `AGENTS.md`.

## 6. Rendering

Symptoms: types look fine internally but pseudocode unreadable.

- Only after NIR/HIR matches intent should you tune [`printer.rs`](../../crates/fission-pcode/src/nir/printer.rs) paths.

## 7. Regression scale-up

When the single function is understood:

```bash
cargo run -p fission-automation -- nir-check --lane nir --functions-limit 50 --no-build --fission-bin ./target/release/fission_cli
```

Attach `benchmark/artifacts/automation/` excerpts to the PR if quality logic changed ([`CONTRIBUTING.md`](../../CONTRIBUTING.md)).
