# Debugging a decompilation failure

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

- Inspect the function with `fission_cli raw-pcode <binary> --addr <address>`. Consult the [Ghidra reference policy](../adr/0005-ghidra-reference-boundary.md) when comparing lift behavior.

## 4. NIR build (`fission-pcode`)

Symptoms: empty bodies, chaotic control flow, telemetry explosions.

- Inspect counters with `fission_cli nir-stats <binary> --addr <address> --json`. See [quality metrics](../QUALITY_METRICS.md) for batch telemetry.
- Canonical counters live in [`crates/fission-midend-core/src/ir/build_stats.rs`](https://github.com/fission-systems/Fission/blob/main/crates/fission-midend-core/src/ir/build_stats.rs).

## 5. Structuring / normalization

Symptoms: duplicated labels, collapsed loops, odd regions.

- Fix in [`crates/fission-pcode/src/midend/structuring/`](https://github.com/fission-systems/Fission/tree/main/crates/fission-pcode/src/midend/structuring) per child `AGENTS.md`.

## 6. Rendering

Symptoms: types look fine internally but pseudocode unreadable.

- Only after NIR/HIR matches intent should you tune [`printer.rs`](https://github.com/fission-systems/Fission/blob/main/crates/fission-pcode/src/render/printer.rs) paths.

## 7. Regression scale-up

Run the relevant crate suite with `cargo nextest run -p <crate>`. For a quality
claim, attach cache-disabled before/after evidence following the
[DecBench guide](../BENCHMARK_DECBENCH.md).
