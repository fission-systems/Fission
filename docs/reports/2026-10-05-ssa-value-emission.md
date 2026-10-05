# SSA value emission: measured regression report

Date: 2026-10-05. Candidate Z; baseline main `493d93d2cdcf1aad908b816270d4d799932dfa3e`.

## Accepted scope

The builder connects admitted scalar SSA values to emitted definitions/reads and transports observable phi values on their actual CFG edges. ABI inputs remain separate from mutable carriers. Original-site producer snapshots supply complete and contained reads; unsigned physical windows retain explicit views through subflow/cast cleanup. A must-definition fixed point validates every reachable edge before an isolated trial is committed. Unsupported inputs, call clobbers, guarded writes, named layouts, entry phis without an entry operand, irreducible graphs and unproven clones retain the existing path.

Entry promotion rejects whole-body renaming after redefinition. Identity provenance stops at arithmetic/load/call or conflicting terminal copy sources; copy SCCs with one value source remain valid. Inferred homogeneous byte/word arrays are not guessed as records. No CLI/JSON/adapter/matcher/denominator/dependency change. Source/DWARF remain evaluator-only.

This is one validated step of #182. It does not close the issue or claim all register lifetimes and type inference are SSA-based. PR #176 and the primary checkout are preserved.

## Fresh 250-function measurement

| Metric | Baseline | Candidate |
| --- | --- | --- |
| Union | 75/250 (30.0%) | 76/250 (30.4%) |
| ged perfect / measured; mean | 65/240; 33.675000000000 | 65/240; 33.620833333333 |
| type_match perfect / measured; mean | 18/228; 0.265775404759 | 19/228; 0.269297467492 |
| byte_match perfect / measured; mean | 1/166; 0.211890045226 | 1/166; 0.212731260763 |
| Recompiles | 133/166 | 133/166 |

Fresh-baseline perfect losses: **0**, including each individual metric. Previously compiling rows lost: **0**. The new Union row is `cronie::O2::crontab::strcmp_until`. Public historical Fission73 is not the baseline used here.

Frozen competitor comparison: local rank **8**; Kuna 88/250 requires 89, hence **13 more**. Fixed published 91-function mask: **23 -> 24/91**, local **joint 4th**. Mask SHA256 `883255f7b14081af9347c6c510b6e36d48a30887b0b28155e9100ab5300cfae3`. These are local projections against the 2026-09-23 export, not current official rankings. Published metric denominators differ; these fresh baseline/candidate denominators are identical.

### All changed scored rows

| Row | Metric | Before | After |
| --- | --- | --- |
| cronie::O2::crontab::strcmp_until | ged | 22.0 | 9.0 |
| bash::O2-noinline::man2html::scan_format | type_match | 0.0 | 0.09090909090909091 |
| cronie::O2::crontab::strcmp_until | type_match | 0.3333333333333333 | 1.0 |
| riot-os::O0::hello-world::hard_fault_handler | type_match | 0.45454545454545453 | 0.5 |
| coreutils::O0::pr::balance | byte_match | 0.16417910447761194 | 0.18181818181818182 |
| cronie::O2::crontab::strcmp_until | byte_match | 0.19148936170212766 | 0.3333333333333333 |
| tar::O2-noinline::tar::xheader_list_append | byte_match | 0.7142857142857143 | 0.6944444444444444 |

Partial losses are disclosed above. For the tar row, the old output selects through an indeterminate `rbp`; the new output initializes its carrier from the incoming value, updates it only on the applicable branch, and stores that value. Its assembly edit distance changes 10 -> 11. The bounded pointer/effect probe below passes 8/8; improved value/effect preservation can still lower an assembly similarity score. No register-allocation explanation is asserted without disassembly evidence.

## Execution and mechanical gates

- Complete benign recovered `strcmp_until` body versus exact published source body: **256/256 at host O0 and 256/256 at O2**. Empty strings, delimiters, unequal/negative results and zero/one/multiple loop iterations are included. Baseline 66/256 diagnostics supply zero for its invented fourth formal; three-argument baseline ABI parity is not claimed. ASCII/host-char coverage is not proof for arbitrary high bytes or raw-binary parity.
- Recovered tar body with identical instrumented allocator/string helpers: **8/8** under both baseline diagnostic sentinels. Branch/default path, high pointer bits, stored fields, and call count/order are checked. Baseline2/8 and6/8 are diagnostics of an indeterminate carrier, not unmodified whole-binary execution.
- Targeted nextest: 87 passed; full nextest: **1,759 passed, 1 preexisting skipped** across pcode, normalize and decompiler. Checks, release native/Linux CLI builds, workspace fmt, Clippy with CI deny-warning flags, fixture-reference and owner-boundary scans pass.
- Synthetic coverage includes actual SSA predecessor reads, immutable formals, narrower lower/upper snapshot views, mixed-width high bits, zero/one/multiple iterations and multiple backedges, final post-loop reads, effectful loads, cyclic parallel-copy fanout, cover-rejected merging, previous-definition reads, conflicting alias sources, skipped edges, invalid plans and unproven clone fallback.
- Both final generations emit250/250 without errors or retries; NIR/HIR/PreHIR/variables match for all250. Wall-clock telemetry is excluded. Earlier X/Y exploration had a concurrent-load timeout; identical-option fresh retry and independent generation succeeded. Those trial histories are retained, not substituted for final Z evidence.

## Paired external Docker regression

Exact clean493d baseline Linux build versus Z; external runner `70e4c6348db412bcde31d17ae5f1854829d1db09`. Identical dev6/fixed unscored32 selection, caches disabled, checkpoint recovered rows0 on both sides. All previously bare-compiling rows retained: dev4/6 and pool8/32.

Dev semantic wrappers fail compilation for all6 before/after (existing typedef/intrinsic issues). Pool28 rows have no wrapper and4 have adapter errors before/after. These results do not demonstrate whole-function runtime parity or new semantic passes.

External partial deltas (go/stop evidence only; not ranking or tuning targets):

- dev kv_lookup::gcc -O0: {"recompilation_score": {"before": 0.3103, "after": 0.25}}
- dev list_sum::gcc -O1: {"recompilation_score": {"before": 0.5385, "after": 0.6364}}
- scale decbench::bash::bash::init_mail_file::gcc -O0: {"ged_score": {"before": 0.0, "after": 13.0}}

The pool structural delta exposes explicit initialized branch carriers where the previous output used a conditional expression plus an indeterminate read. Statement-content/conditional-expression blindness of GED limits interpretation. The measured structural penalty is retained and disclosed; the pool is not used to choose a rule or repair output.

## Reproduction and pinned artifacts

Artifacts: `benchmark/artifacts/decbench/2026-10-05-ssa-lifetimes/` in the primary checkout (ignored generated data). `accepted-z.json` contains every acceptance assertion; `candidate-z-{rank,all-overlays,provenance,fixed91}.json`, `baseline-{rank,all-overlays,fixed91}.json`, `external-z-*`, `determinism-z.json`, execution sources/results, logs and source patches retain the evidence. Reproduction helpers are saved in `reproduction-scripts/`.

Native baseline CLI SHA256 `708d9f430076ce70d03ad4d56e25b52f9c7b4e057a2a93e6fae04c729727a63d`.
Native Z CLI SHA256 `6f5415804cef8575f7811f009eb8394a7dc47705c82f3127a62ce2a759e8dfb7`.
Linux Z CLI SHA256 `6524b9213d852756da91543430b5d0c543fa76661e75e097930e68b8cbf6886c`.
Linux source fingerprint `5520d114079da1286cfcdd557a957cd2d696e279d0c1cff510035ec8792060aa`.

Evaluator base `cc95ae6f4386e0ad496ebc9548431bda0b60ce3c`, with the preexisting custom adapter/scoring files held unchanged; their full digests are recorded. Evalkit manifest `1e8d3a28810bd05ee40d1f30aa4ee159a425d00094214fe0847860d78239a7ba`; all223 stripped input hashes match. One input absent from the newer kit was restored by content hash, not identity guessing. No corpus executable was run. Only the bounded benign recovered/source functions above were executed.

```bash
export CARGO_TARGET_DIR=/path/to/isolated/native-target
cargo nextest run -p fission-pcode -p fission-midend-normalize -p fission-decompiler
cargo check -p fission-pcode -p fission-decompiler
cargo build -p fission-cli --release
cargo fmt --all --check
cargo clippy -p fission-pcode -p fission-midend-normalize -p fission-decompiler --locked -- -D warnings -A clippy::style -A clippy::complexity
```

The saved pipeline_z.py runs fresh generation twice, packages the existing adapter metadata, force-ingests into a separate evaluator tree, then runs uncached GED/types/bytes. `external_z.py` follows `docs/BENCHMARK_DOCKER.md` against the local Linux bundle and fixed scale_pool.py. Paths in those local artifact scripts identify the retained evaluator/input/toolchain snapshots; adapt operator paths without changing hashes or scoring contracts.

Ghidra vendor merge-cover behavior was consulted as an invariant reference only: intersecting lifetimes must not be forced to share storage; copies remain necessary. No vendor code, bindings or runtime dependency is used.
