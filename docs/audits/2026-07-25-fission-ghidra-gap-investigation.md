# Fission ↔ Ghidra gap investigation

- **Date:** 2026-07-25
- **Ghidra reference tree:** `vendor/ghidra/ghidra-Ghidra_12.0.4_build` (+ `ghidra_12.0.4_PUBLIC`) — **reference only** ([ADR 0005](../adr/0005-ghidra-reference-boundary.md))
- **Measurement primary:** `/Users/sjkim1127/fission-benchmark` (official `results/latest.json`, layered parity telemetry)
- **Related:** [2026-07-10 decompiler problem inventory](2026-07-10-decompiler-problem-inventory.md), [ADR 0014](../adr/0014-dir-product-vs-prestructure-substrate.md), [BENCHMARK_DOCKER.md](../BENCHMARK_DOCKER.md)

## Executive summary

| Layer | Fission vs Ghidra (measured) | Primary gap owner |
|-------|------------------------------|-------------------|
| Assembly / decode | **Matched** on parity sample | — |
| Raw p-code shape | **Matched** (strict full match rate 1.0 on n=40) | — |
| CFG shape | **~match** (1 `block_count` mismatch in telemetry sample) | CFG edge cases |
| Function inventory | Primary stage; treat Ghidra inventory as reference | loader / discovery |
| **Decompiled product quality** | **Large residual** | midend raise → structure → print |

**Headline:** Shared **semantic foundation** (assembly + p-code) is already near Ghidra on the layered-parity corpus. The residual that hurts ranking and source similarity is **above p-code**: materialize/bindings, types, control structure, expression cleanup, and printable C shape — not “missing Ghidra runtime inside Fission.”

Do **not** close the product gap by copying Ghidra Decompile C++ into `crates/`. Use Ghidra 12.0.4 as an **algorithm / invariant oracle**, implement in Rust owners, remeasure.

## Policy boundaries (non-negotiable)

1. **ADR 0005** — vendor Ghidra is reference-only; no runtime/build dependency on `vendor/ghidra`.
2. **ADR 0006 / Core Rule 10** — quality claims need measured before/after (fission-benchmark local docker or source-semantic row), not unit tests alone.
3. **ADR 0007** — when prompting external models, describe structural failure patterns; redact row ids/addresses unless local-only.
4. **ADR 0014** — hybrid validation (emu/solver) compares against **p-code foundation**, not print-NIR or HIR text alone.
5. **Presentation** — do not chase Ghidra-specific readability quirks as success criteria; prefer source/oracle semantics + measured readability proxies.

## Measurement surfaces (use these; do not invent parallel oracles)

| Surface | Repo / command | What it answers |
|---------|----------------|-----------------|
| Multi-decomp ranking + source similarity | `fission-benchmark` `runner/runner.py --decompilers fission,ghidra` | Product quality vs Ghidra on same rows |
| Layered IR parity | `python -m runner.run_parity --corpus dev --decompilers fission,ghidra` | Assembly / pcode / CFG / FD vs Ghidra |
| Parity smoke gate | `python scripts/check_parity_smoke.py results/telemetry/latest.json` | CI-style reliability of parity batch |
| Golden repros freeze | `scripts/extract_golden_repros.py` + `benchmark.golden_repros.run` | Known IR mismatches pinned |
| Local NIR/HIR text gate | `scripts/quality/golden_corpus_check.py` | Local regression only (not Ghidra) |
| Focused decomp delta | `scripts/quality/local_decomp_observe.py` | Single-function before/after |
| Ghidra tree | `vendor/ghidra/ghidra-Ghidra_12.0.4_build` | Manual algorithm reference |

Official dashboard split: `/fission-vs-ghidra` = shared IR parity (not ranking); ranking hub must not mix IR match rates into multi-tool scores ([BENCHMARK_OPERATING.md](file:///Users/sjkim1127/fission-benchmark/docs/BENCHMARK_OPERATING.md)).

## Baseline snapshot (official, 2026-07-21)

**Artifact:** `fission-benchmark/results/latest.json`  
**Run:** `run_id=46f33b47-…`, `official=true`, corpus `dev`, profile `realistic` / `core_c_pe`, finished `2026-07-21T19:51:08Z`.

### Product scores (paired fission vs ghidra, n=216 rows each)

| Metric | Fission | Ghidra | Gap (G−F) |
|--------|---------|--------|-----------|
| Mean `source_similarity` | **0.146** | **0.382** | **+0.236** |
| Mean `semantic_score` | **0.548** | **0.769** | **+0.221** |

### Oracle fail categories (same paired set)

| `fail_category` | Fission | Ghidra |
|-----------------|---------|--------|
| (empty / ok path) | 99 | 164 |
| `assertion_fail` | 47 | 13 |
| `compile_error` | 44 | 22 |
| `runtime_error` | 10 | 14 |
| `timeout` | 8 | 1 |
| other | 8 | 2 |

Interpretation: Fission more often emits C that **fails differential oracle compile or assertions**, even when some rows still pass a fraction of cases. That is a **product midend / printer** problem class, not a p-code lift miss on the parity sample.

### Largest source-similarity gaps (Ghidra ≫ Fission)

Illustrative top residuals (full table in artifact; not an implementation hit list by address):

| Pattern family | Example functions | Notes |
|----------------|-------------------|-------|
| Simple arith presentation | `add_ints`, `mul_ints` | Often **semantic_score=1.0 both** — gap is **readability / source shape**, not behavioral oracle |
| Checksum / hash loops | `checksum`, `rolling_hash32` | High sim gap; mix of type/cast noise and loop form |
| Data structures | `accumulate_pairs`, `sum_array`, `find_pair_value` | Pointer/struct/index recovery + assertion fails |
| Tables / search | `kv_lookup`, `linear_search` | Control + memory layout |
| Matrices | `matrix_multiply` | `compile_error` on Fission sample row |

**Important split for triage:**

- **Semantic residual** (`semantic_score` F ≪ G, or `assertion_fail` / `compile_error`): own in materialize / types / structure legality.
- **Presentation residual** (semantic ≈1.0 both, source_sim still low): own in HIR presentation / printer under ADR 0011; do not “fix” by copying Ghidra text style.

### Layered IR parity (telemetry sample)

**Artifact:** `fission-benchmark/results/telemetry/latest.json` (+ `results/pcode_parity/latest.jsonl`).

| Stage | Status (sample) |
|-------|-----------------|
| `assembly_parity` | 40/40 match |
| `pcode_parity` | 40/40 match; **strict_full_match_rate = 1.0**, opcode sequence match 1.0 |
| `cfg_parity` | 39 match + 1 `block_count` mismatch (in aggregate mismatch_kind) |
| Publishable comparable match rate | **0.9922** (assembly + pcode + cfg + function_discovery) |

**Conclusion:** Closing the Ghidra **product** gap by re-litigating SLEIGH/p-code is the wrong ROI for most residual rows. Prefer midend owners (see inventory F1–F6).

## Gap taxonomy → Fission owner → Ghidra reference (read-only)

Map problems to **canonical owners**. Ghidra paths under `vendor/ghidra/ghidra-Ghidra_12.0.4_build` are for **reading algorithms only**.

| ID | Gap class | Fission owner | Ghidra reference areas (indicative) | Verify with |
|----|-----------|---------------|--------------------------------------|-------------|
| G0 | Decode / p-code lift | `fission-sleigh`, `fission-pcode` pcode | Sleigh, pcode emit | `pcode_parity`, `assembly_parity` |
| G1 | Function discovery / bounds | `fission-loader`, `fission-static` discovery | function manager / analyzers | `function_discovery` stage |
| G2 | Binding / value identity / params | builder materialize | High-level decompiler variable / stack | semantic oracle + local observe |
| G3 | Return / cmov / flag recovery | materialize + terminator | rule actions / conditional moves | control_flow rows |
| G4 | Structuring (if/loop/switch/goto) | `fission-midend-structuring` | Decompiler block graph / structure | goto_count, nested structure, oracle |
| G5 | Type / pointer / array / struct | type recovery + static facts | data type manager / recovery | type_parity (extension), source_sim |
| G6 | Print / HIR presentation | `render` + ADR 0011 | — (do not clone Ghidra print quirks) | NIR/HIR dual layer, readability proxies |
| G7 | Hybrid validation (future DIR product) | emu + solver vs **p-code** | — | ADR 0014; not Ghidra Decomp |

Existing ROI table in the 2026-07-10 inventory (F1–F6) remains the **implementation priority** inside G2–G5.

## What “improve” means (ordered)

1. **Rebaseline (verify current tree)**  
   - Refresh local Fission docker image (`prepare_local_fission.sh` + compose local).  
   - Run `runner.py --corpus dev --decompilers fission,ghidra` with caches disabled for claims.  
   - Run `run_parity --limit 20` (or full) and `check_parity_smoke`.  
   - Record SHA + artifact paths in this audit’s “Remeasurements” section.

2. **Pick one measured residual family** (not “be more like Ghidra”).  
   Prefer rows where **semantic** or **fail_category** differs, not pure style.  
   Example families from baseline: `checksum` / `accumulate_pairs` / `assertion_fail` clusters.

3. **Owner proof + invariant** (ADR 0006 proposal template).  
   Cite Ghidra behavior only as cleanroom reference for the invariant.

4. **Implement at owner** → crate tests → **remeasure same path**.

5. **Regression** — golden_corpus_check + broader smoke; no holdout collapse.

6. Optional later: DIR-product path validates candidates against **p-code** (ADR 0014), with Ghidra as secondary human reference only.

## Verification checklist (operators)

```bash
# A) Local CLI regression (no Ghidra)
cd /Users/sjkim1127/Fission
cargo build -p fission-cli --profile quick-release
python3 scripts/quality/golden_corpus_check.py check

# B) External semantic + Ghidra product compare
cd /Users/sjkim1127/fission-benchmark
export FISSION_ROOT=/Users/sjkim1127/Fission
./scripts/prepare_local_fission.sh
docker compose -f docker-compose.yml -f docker-compose.local.yml \
  --profile local up -d --build fission
docker compose -f docker-compose.yml up -d ghidra   # if not running
export FISSION_ENDPOINT="http://localhost:${FISSION_HOST_PORT:-8007}"
# Focused product compare (adjust limit)
python runner/runner.py --corpus dev --decompilers fission,ghidra --limit 40 \
  --output "results/local_gap_$(date +%Y%m%d)_fission_ghidra.json"

# C) Layered IR parity
python -m runner.run_parity --corpus dev --limit 20 --decompilers fission,ghidra
python scripts/check_parity_smoke.py results/telemetry/latest.json
```

**Pass criteria for “gap investigation verified” (process):**

- [ ] Product compare artifact exists for current `Fission` SHA (not only 2026-07-21 official).
- [ ] Parity smoke green or mismatches filed as G0/G1 golden repros.
- [ ] Top residual families classified as semantic vs presentation.
- [ ] Next implementation work has a single owner + ADR 0006 proposal anchor.

**Pass criteria for “gap improved” (quality claim):**

- [ ] Same corpus row(s) show measured movement (semantic / fail_category / source_sim as agreed).
- [ ] No unexplained regression on smoke/holdout policy.
- [ ] Report explicitly: mechanical change vs quality improved.

## Remeasurements

| When | Fission SHA | Artifact | Notes |
|------|-------------|----------|-------|
| 2026-07-21 | official release bake | `results/latest.json` | Historical product baseline (core_c_pe, n=216 pairs) |
| 2026-07-25 a | docker Fission `0.1.4` + Ghidra 12.0 | `run_parity --limit 3` | 15/15 IR match (pre-rebake) |
| 2026-07-25 b | **`c8f9f24a`** local bundle fp `5b24da74…` / Fission **0.1.6** + Ghidra **12.0** | `results/local_gap_c8f9f24a_fission_ghidra.json` + `run_parity --limit 20` | Pre-fix product baseline (local diagnostic). |
| 2026-07-25 c | **`ee492fb6`** local `0.1.6` + Ghidra **12.0** | `results/local_gap_ee492fb6_focused/*.json` | After P0 return/locals + P1 stride/field + found-path break→return. Focused functions only. |
| 2026-07-25 d | **`912b4347`** local `0.1.6` + Ghidra **12.0** | `results/local_gap_912b4347_focused/*.json` | After named `fission_agg` field typedefs + Aggregate Index cast. |
| 2026-07-25 e | **`1ea1a26f`** local `0.1.6` | `results/local_gap_1ea1a26f_focused/*.json` | Orphan goto→`i++; continue`; clang Pair/KV compile_error cleared. |

### Current-tree product remeasure (`c8f9f24a`, 2026-07-25)

**Setup:** `prepare_local_fission.sh` → compose local Fission healthy (`release_version=local-c8f9f24a`).  
**Runner:** `runner.py --corpus dev --decompilers fission,ghidra --run-mode local` (~1020s).  
**Note:** local profile is **diagnostic** (wider than official `core_c_pe`); pair counts differ from 2026-07-21 official.

| Slice | n pairs | Mean source_sim F / G | Mean gap (G−F) | Mean semantic F / G |
|-------|---------|------------------------|----------------|---------------------|
| All paired | 372 | 0.156 / 0.365 | **+0.209** | 0.439 / 0.708 |
| Core-ish C (excl. go/rust name noise) | 334 | 0.153 / 0.384 | **+0.231** | 0.462 / 0.780 |
| Official 2026-07-21 (reference) | 216 | 0.146 / 0.382 | +0.236 | 0.548 / 0.769 |

**Fission fail_category (core-ish C, n=334):** empty 129 · `compile_error` 97 · `assertion_fail` 71 · `runtime_error` 16 · `timeout` 10 · `adapter_error` 11.  
**Ghidra (same):** empty 257 · `compile_error` 35 · …

**Classification (core-ish C):**

| Bucket | Count | Definition used |
|--------|------:|-----------------|
| Semantic residual | **165** | Ghidra semantic − Fission ≥ 0.34, or Fission fail + Ghidra sem≈1 |
| Presentation residual | **55** | Both semantic ≈ 1.0 and source_sim gap ≥ 0.25 |
| Both semantic ≈ 1.0 | 94 | Mean source_sim gap still **+0.29** (readability / shape) |

**Highest mean semantic gap by function (current tree):**  
`reverse_string`, `matrix_multiply`, `linear_search`, `bounded_checksum`, `kv_lookup`, `find_pair_value`, …

### Layered IR parity (`run_parity --limit 20`, concurrent with product run)

| Stage | match_rate (comparable) | Coverage note |
|-------|-------------------------|---------------|
| assembly / pcode / cfg | **1.0** where fetched | Some `fetch_error` under dual load (usable_coverage ~0.80) |
| function_discovery | **1.0** on attempted | Lower coverage (fetch_error under load) |
| ir_invariants | **1.0** | 20/20 |

**No IR mismatches observed** on successful fetches — product gap remains above p-code.

### Concrete semantic residual anchors (code-level)

Pulled from `local_gap_c8f9f24a_fission_ghidra.json` (not for overfitting — invariants only).

| Family | Example row | Fission failure | Owner hint | Ghidra contrast |
|--------|-------------|-----------------|------------|-----------------|
| **Missing return value** | `linear_search` gcc -O2 | `return;` in non-void → **compile_error**, sem=0 | G3 terminator / return materialize | Returns index on hit |
| **Undeclared local** | `matrix_multiply` clang -O0 | `local_18 = param_3` never declared → **compile_error**, sem=0 | G2 binding / local emission | Clean triple loop, float\* |
| **Struct field / stride** | `accumulate_pairs` clang -O0 | Wrong pair field addressing → **assertion_fail** 2/5 | G5 type + pointer | `pairs[i].key * .value` |
| **KV value path** | `kv_lookup` gcc-m32 -O0 | Wrong value load / always -1 path noise → **assertion_fail** 2/6 | G5 + control | Returns `items[i].value` |

**Presentation anchors (both sem=1.0):** `checksum`, `sum_array`, `mul_ints`, `add_ints` — queue under ADR 0011 / printer, not midend semantic P0.

### Landed fixes (2026-07-25, commits on `main`)

| Commit | Change |
|--------|--------|
| `446dc4b9` | Live primary return on stack-target RET; keep write-only stack locals; ADR 0014 |
| `cb94c87a` | Wide-stride `base[i].field_0`; sub-element PtrOffset rescale (+1→+4) |
| `ee492fb6` | Found-path `result=…; break` → `return result` before sentinel `return -1` |
| `912b4347` | Named fields on `fission_aggN` typedefs; cast Index base for Aggregate elems |
| `867f4380` / `1ea1a26f` | Orphan loop gotos → `continue` (+ inject `i++` on bound-break search loops) |

### Focused remeasure (`ee492fb6` vs Ghidra 12.0)

Artifacts under `fission-benchmark/results/local_gap_ee492fb6_focused/`.

| Function | Notable Fission movement (selected variants) |
|----------|-----------------------------------------------|
| `linear_search` | **gcc -O2/O3/O0, m32-O0: semantic 1.0 (6/6)** — was compile_error / empty return class on O2 |
| `find_pair_value` | **gcc -O0, m32-O0: semantic 1.0 (5/5)**; clang still compile_error |
| `kv_lookup` | **gcc -O0, m32-O0: semantic 1.0 (6/6)** — was assertion_fail class on m32-O0 |
| `accumulate_pairs` | **gcc O0/O1/O2, m32 O0/O2: semantic 1.0**; clang-O0 still compile_error |
| `matrix_multiply` | Undeclared-local class improved; post-`1ea1a26f` residual was **missing `c[]` store (Index DSE)** + **float→uint type loss**. **`3ca1a786` focused remeasure:** **clang -O0 sem=1.0 (5/5)** (was compile_error/sem=0); gcc -O0 sem=0.20 (1/5 assertion_fail); other variants still compile_error/adapter. Store+float recovery is a measured clang-O0 quality win; residual structure/return on other opts open. |

**Not a full-corpus claim** — focused rows only; do not promote to Pages.

## Open risks

- Local docker Fission can lag monorepo — this remeasure used **`c8f9f24a`** bake; re-run after further commits.
- Local **diagnostic** matrix ≠ official `core_c_pe`; use absolute metrics carefully when comparing to Pages.
- Parity `fetch_error` under concurrent product load — re-run parity alone before filing G0 bugs.
- `source_similarity` is not pure semantics; pair with `semantic_score` / fail_category.
- Do not optimize solely against Ghidra text; overfit firewall still applies.

## Immediate next actions (recommended)

1. ~~Re-bake local Fission + product compare~~ **Done** (`c8f9f24a`).
2. ~~IR parity sample~~ **Done** (match where fetched; re-run solo if filing lift bugs).
3. **Next:** ADR 0006 proposal for **empty non-void return** (or undeclared local) with synthetic + row remeasure.
4. Keep presentation residuals (`checksum`/`mul_ints` with sem=1) on HIR presentation queue (ADR 0011).
