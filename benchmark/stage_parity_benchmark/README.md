# Stage parity benchmark

Mid-stage parity tooling belongs **only** under this directory. Do **not** wire it into default `cargo test` / workspace unit tests; fixture paths and fixed addresses live in manifests here (or sibling benchmark manifests), not in `crates/*/src/**/*.rs`.

## Oracle join

Treat Ghidra-side facts from [`benchmark/ghidra_oracle_benchmark/`](../ghidra_oracle_benchmark/README.md) (`rows[].ghidra`) as the single oracle source. Join Fission-side stage dumps (future JSON exports) or existing benchmark rows (`preview_build_stats`, etc.) against that oracle.

**Join keys**

1. Prefer scoping by binary ID (`binary_id`) when present.
2. Match function addresses using the same normalized string key as Grand Finale (`normalize_address`).
3. Use names only as a debugging hint (if ambiguous, trust the oracle manifest `match_evidence`).

## Owner bucket examples

Debugging labels categorize suspected loss causes (field names may evolve per tool).

| Symptom | Ghidra oracle | Fission observation (example) | `owner_bucket` candidate |
|---------|----------------|-------------------------------|---------------------------|
| Call target mismatch | `call_targets` contains `printf` | Call-recovery sub-path fallback counts rise | `call_target_missing` |
| xref too low | High `xref_out_count` | Internal xref table sparse | `xref_pipeline_gap` |
| Signature mismatch | Stable `signature` / `param_count` | Missing type hints | `type_facts_stage` |

When raw p-code agrees but final C diverges, oracle **string refs, external call counts, and parameter counts** help localize which stage dropped facts.

## Xref metrics lane (additive)

`stage_metrics.build_stage_report` now emits `stages.xrefs` with integer counters defaulted to **zero**.
Benchmark runners may populate top-level `xref_metrics` on each row payload once Rust exports land; compare against Ghidra reference facts via [`benchmark/ghidra_oracle_benchmark/`](../ghidra_oracle_benchmark/README.md) using the same binary/function join keys as above.

## Related tooling

- Latency / timeout cross-summary: [`benchmark/timeout_distribution_benchmark/`](../timeout_distribution_benchmark/README.md)
