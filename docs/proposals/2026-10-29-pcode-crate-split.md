# Splitting `fission-pcode` to cut rebuild fan-out (2026-10-29)

Status: proposal. No code is moved by this document.

This is a **mechanical** refactor proposal. It makes no decompiler-quality
claim ([AGENTS.md](../../AGENTS.md), Core Rule 10): the only behavioural gate is
that DecBench output stays byte-identical (see "Verification").

## Why

A pull request that touches Rust rebuilds every crate above the one it
changed. In CI the dependency cache already absorbs the external crates, so
the compile time of an ordinary PR is the workspace crates being rebuilt. This
proposal asks where that fan-out comes from, and which splits of
`fission-pcode` (137k lines, the crate with the largest own cost) would
actually shrink it. It continues [ADR 0012](../adr/0012-midend-rename-and-crate-extraction.md),
whose phase D ("owner extraction") is still in progress.

## How this was measured (and its limits)

All numbers below were measured on one machine: macOS, `rustc 1.97.0`, `main`
at `266e3f4a8`, `cargo test --no-run --workspace --locked --timings` with
`CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=4` to resemble CI. The scripts are
local (the `benchmark/` tree is not tracked); they are described, not
committed.

* **Unit time** is cargo's per-unit duration from the `--timings` report, split
  into `lib`, `lib-test` (the crate compiled again with `cfg(test)`) and
  `other` (integration tests, benches, build scripts). It is CPU time summed
  over units, not wall time.
* **Replay** takes the 183 non-merge commits on `main` from 2026-09-24 to
  2026-10-29 that touch Rust or `Cargo.toml`, maps each touched file to a crate
  (or to a candidate crate), and charges the unit time of every unit cargo would
  rebuild: the changed crate, every crate that depends on it, and, through a
  dev-dependency, only the dependent's test units. A test-only file charges
  only that crate's test units.
* **Split costs are estimates.** The measured `fission-pcode` cost (108.3 s:
  lib 41, lib-test 66, other 1) is divided over the parts by line count
  (`lib-test` also carries the 30,419 lines of in-crate tests, which belong to the
  remainder). The three parts sum back to the measured 108.3 s.
* The replay models the leaf as the 6,559 lines of `pcode`, `arch` and `cfg`
  (5.7 s) and `fission-cspec` as the 5,127 lines of `cspec` (4.4 s). The extra
  `midend/cfg.rs` (1,209 lines) and `abi.rs` (597 lines) listed under the
  candidates would add roughly 1 s and 0.5 s; they do not change the ranking.
* Not modelled: nextest *execution* time (CI: 258 s for 3,270 tests), link
  time (small here: 8 s over 85 links, but this is macOS `ld`, not the Linux
  linker CI uses), and the wall-clock effect of fewer units on a 4-vCPU runner.
* A commit is treated as one CI build. Cargo rebuilds a dependent on any
  source change of its dependency, including private changes, so no saving is
  assumed from "the interface did not change".

## Observed baseline

### Where the time is

| | Cold build, CI-like | After touching `fission-pcode` |
|---|---|---|
| Wall time | 232 s | 92 s |
| Unit time, workspace crates | 519 s | 305 s |
| Unit time, external crates | 389 s | 0 s (cached) |
| Linking | 172 links, 11.9 s (1.3 %) | 85 links, 8.0 s (2.6 %) |

External dependencies are not the lever for an ordinary PR; the workspace
crates are. `fission-pcode` alone is 108.3 s of the 519 s.

### What the history changes, and how far it propagates

Replaying the 183 commits (rebuild-on-change = unit time rebuilt when that
crate's source changes):

| Crate | Commits touching it | Rebuild-on-change | Share of total replay cost |
|---|---:|---:|---:|
| `fission-midend-normalize` | 35 | 389 s (75 %) | 18 % |
| `fission-loader` | 24 | 472 s (91 %) | 15 % |
| `fission-pcode` | 31 | 350 s (68 %) | 14 % |
| `fission-signatures` | 13 | 493 s (95 %) | 8 % |
| `fission-sleigh` | 18 | 309 s (60 %) | 7 % |
| `fission-static` | 19 | 289 s (56 %) | 7 % |
| `fission-midend-prehir` | 12 | 413 s (80 %) | 6 % |
| `fission-midend-structuring` | 12 | 370 s (71 %) | 6 % |
| `fission-midend-core` | 9 | 420 s (81 %) | 5 % |
| `fission-emulator` | 23 | 136 s (26 %) | 4 % |

Two structural facts explain why `fission-pcode` is on the critical path of so
many rebuilds:

1. **Crates that only need the IR still depend on the whole crate.**
   `fission-sleigh`, `fission-emulator`, `fission-dir` and `fission-static`
   import almost nothing from `fission-pcode` except p-code IR types (below), but
   because they depend on the crate, a change in the 114k-line `midend` rebuilds
   them. `fission-emulator` and `fission-dir` additionally reach `fission-pcode`
   through `fission-static` and `fission-decompiler`.
2. **A dev-dependency cycle** (`fission-pcode` dev-depends on `fission-sleigh`
   and `fission-static`, which depend on `fission-pcode`). Cargo allows it, but
   it means a change in `sleigh`, `static`, or anything beneath them rebuilds
   `fission-pcode`'s test units (lib-test is 66 s, larger than the 41 s lib,
   because of the 30,419 lines of in-crate tests).

29 % of the replayed commits (53 of 183) touch a crate with ten or more
dependents (`core`, `loader`, `signatures`, `midend-core`, `midend-prehir`).
Splitting `fission-pcode` cannot help those; see "Beyond this proposal".

## Module dependency graph inside `fission-pcode`

Lines of Rust per group, and the references that cross groups (non-test code,
counted from `crate::…` paths):

| Group | Lines | References to other groups |
|---|---:|---|
| `midend::builder` | 67,301 | `pcode` ×230, `midend` root ×32, `structuring` ×23, `arch` ×14, `midend::cfg` ×11, `cspec` ×10, `support` ×8 |
| `midend::tests` (`cfg(test)`) | 30,419 | builder internals |
| `render` | 16,099 | `midend` print functions ×25 |
| `pcode` (IR, optimizer, graph) | 5,946 | **none** (no workspace-crate imports; one `pub(crate)` item) |
| `midend::structuring` | 5,730 | `midend` root ×8, `ir` ×5, `pcode` ×4, `builder` ×2 |
| `midend::cspec` | 5,127 | `midend` root ×20 (`NirRenderOptions`, a `HashMap` alias), `arch` ×3 |
| `midend::pass` | 1,602 | `structuring` ×10, `builder` ×2 |
| `midend::cfg` | 1,209 | `pcode` ×7, `cfg` ×3, `builder` ×1 (`resolve_lifted_direct_call_target`) |
| `midend::abi` | 597 | `pcode` ×4, `cspec` ×1, `arch` ×1 |
| `cfg` | 343 | `midend::structuring_cfg_edges` ×1 |
| `arch` | 270 | none |

Cycles: `render` calls `midend` print functions ×25 and `midend` (orchestrate,
mod) calls `render` ×8.

### What each dependent imports from `fission-pcode`

Use trees were expanded, so grouped imports are counted. Comments are excluded.

| Dependent | IR types (`pcode`) | `cfg` | `midend` / render |
|---|---:|---:|---|
| `fission-sleigh` | 43 | 4 (`AddressCfgSnapshot`, `AddressEdge`) | 0 |
| `fission-emulator` | 41 (`ir`) | 0 | 0 |
| `fission-dir` | 14 | 0 | 0 |
| `fission-static` | 29 | 0 | 8: `structuring_cfg_edges` ×2, `cspec` ×2, `RegisterNamer`, `infer_entry_register_param_arity`, `seed_nir_render_options`, `NirRenderOptions` |
| `fission-decompiler` | 9 | 0 | 29: `cspec`, `PreHir*`, `build_raw_hir`, `NirFunctionHints`, … |

Only `fission-decompiler` genuinely needs the pipeline. The other four need the
IR, the address-CFG snapshot, and (for `static`) a handful of cspec/abi helpers.

## Candidates

Names are working names.

### A. `fission-pcode-ir`: the p-code IR leaf

* **Contents:** `src/pcode` (5,946), `src/arch` (270), `src/cfg` (343) and
  `midend/cfg.rs` (1,209), about 7.8k lines.
* **Why it is clean:** `pcode` and `arch` have no workspace imports; `src/pcode`
  has one `pub(crate)` item and no deep paths used from `midend` or `render`.
* **Prerequisites:** (1) `cfg/export.rs` calls `midend::structuring_cfg_edges`,
  which lives in `midend/cfg.rs`; moving both resolves it. (2) `midend/cfg.rs`
  calls one builder function (`resolve_lifted_direct_call_target`); relocate or
  copy that helper. (3) `fission-pcode` re-exports the moved items under the same
  paths (`fission_pcode::{Pcode*, Varnode, arch, cfg, ir}`), so the five
  dependents keep compiling unchanged.
* **Fan-out effect alone:** the leaf (estimated 5.7 s) no longer rebuilds when a
  lower-layer crate changes. Replay: **4.7 % less rebuild work** (see table).
  By itself it is weak, because `sleigh`, `emulator` and `dir` still reach
  `fission-pcode` through `static` and `decompiler`.
* **Risk: low.** Public paths are preserved by re-exports. Watch for private
  helpers shared with `midend` (none found), `#![forbid(unsafe_code)]` and
  serde derives in the new crate, and benches/examples that import by path.

### B. `fission-cspec`: calling-convention / register model

* **Contents:** `midend/cspec` (5,127), `midend/abi.rs` (597,
  `infer_entry_register_param_arity`) and `seed_nir_render_options`
  (`midend/mod.rs`). About 5.7k lines plus a small function.
* **Dependencies:** `fission-midend-core` (`NirRenderOptions`), `fission-loader`
  (`LoadedBinary`), the leaf from A; `RegisterNamer` stays re-exported.
* **Prerequisite and ADR impact:** [ADR 0012](../adr/0012-midend-rename-and-crate-extraction.md)
  says `seed_nir_render_options` and the other p-code adapters remain in
  `fission-pcode`. This candidate moves one of them (not
  `nir_admission_facts_from_pcode`), so the ADR needs an amendment.
* **Fan-out effect alone:** small, about 0.6 % beyond A (its 4.4 s estimated
  cost). Its value is that it is what lets `fission-static` stop depending on
  `fission-pcode` (candidate S).
* **Risk: medium.** New dependency on the loader from a crate that today sits
  inside `fission-pcode`; cspec behaviour feeds register naming everywhere, so
  the byte-identical gate must be run on both O0 and O2 rows.

### S. Decouple `fission-static` from `fission-pcode` (requires A and B)

`fission-static` uses 29 IR references and the eight helpers listed above. After A
and B every one of them lives in the leaf or `fission-cspec`, so `static`
(including its dev-dependency on `fission-pcode`) can depend on those two
crates instead. This is a mechanical import change.

* **Why it matters:** `static` is on the path to `sleigh` (dev), `emulator` and
  `dir`; cutting it removes them from the rebuild set of a `fission-pcode`
  change.
* **Fan-out effect (A + B + S):** a change in `fission-pcode` rebuilds
  **175 s instead of 350 s**, and only `cli`, `decompiler`, `dir` and `pcode`
  (before: also `dynamic`, `emulator`, `script`, `sleigh`, `static`). Replay:
  **19.5 % less rebuild work**, median commit 350 s → 213 s.
* **Risk: medium.** Pays off only together with A and B. If a helper turns out to
  need `PreviewBuilder` internals, S is partly blocked; `cargo check -p
  fission-static` after the import switch is the test.

### F. Remove `fission-pcode`'s dev-dependencies on `sleigh` and `static`

Only one file uses them:
`crates/fission-pcode/tests/cfg_ghidra_parity_fixtures.rs`. Moving it into
`crates/fission-sleigh/tests/` (which already dev-depends on `static`) removes
both dev edges. With A done it needs only the leaf (`AddressCfgSnapshot`,
`structuring_cfg_edges`).

* **Fan-out effect alone:** 3.2 % less rebuild work: a change in `sleigh`
  stops rebuilding `fission-pcode`'s 66 s test unit (309 s → 242 s per change).
  It adds to A+B+S: 22.4 % in total.
* **Risk: low.** One file, test-only. A dev cycle between `sleigh` and `static`
  remains and is out of scope.

### Considered and not recommended

* **Extract `render` (16,099 lines).** `render` and `midend` call each other
  (25 and 8 references), and `fission-pcode` would still depend on a `render`
  crate, so a render change would rebuild `fission-pcode` anyway: no fan-out gain.
* **Extract `builder` (67,301 lines).** `structuring` and `pass` call back into it
  (2 + 2 references), `PreviewBuilder` implements `StructuringHost`, and
  `fission-decompiler` needs the pipeline anyway, so no dependent would be
  isolated. High risk, no measured gain.
* **Move the 30,419 in-crate test lines to `tests/`.** Each file becomes its own
  test binary with its own link, and the tests use crate-private items.

## Combined effect (model, 183 commits)

| Candidates | Total rebuild work | Mean / commit | Median | p90 | Saved vs now |
|---|---:|---:|---:|---:|---:|
| none (current) | 57,327 s | 313.3 s | 350.4 s | 472.3 s | |
| A | 54,617 s | 298.5 s | 344.5 s | 446.4 s | 4.7 % |
| A + B | 54,289 s | 296.7 s | 340.0 s | 446.4 s | 5.3 % |
| F | 55,517 s | 303.4 s | 350.4 s | 472.3 s | 3.2 % |
| A + B + S | 46,156 s | 252.2 s | 213.2 s | 446.4 s | 19.5 % |
| A + B + S + F | 44,498 s | 243.2 s | 213.2 s | 446.4 s | 22.4 % |

* The p90 does not move: commits that touch the bottom of the graph (`core`,
  `loader`, `signatures`, `midend-*`) rebuild almost everything in every variant.
* These are model results on estimated split costs. They are an argument for the
  order of work, not a promise: the success criterion below is a re-measurement.
* The saving is in *compile* unit time. CI test selection is by affected
  crate, so a smaller affected set should also shorten test execution; that was
  not measured.

## Beyond this proposal

Splitting `fission-pcode` addresses about a fifth of the replay cost at best. The
rest sits lower: `fission-midend-normalize` (18 %), `fission-loader` (15 %) and
`fission-signatures` (8 %) are changed often and almost everything depends on
them. Before proposing anything there, check what those commits actually touch
(for example whether `loader` or `signatures` changes are data/format code that
the pipeline above does not read), as a separate measured proposal.

## Order of work

One crate per pull request, each independently revertable:

1. **F** (one file, no dependencies on the others). Re-measure.
2. **A**, with re-exports so no dependent changes. Re-measure.
3. **B**, with the ADR 0012 amendment.
4. **S**: switch `fission-static`'s imports. This is the step that moves the
   numbers; if it is blocked, stop and reassess before B is worth keeping.

## Verification

For every step:

* **Behaviour unchanged:** the 250-row DecBench sample-set gate, run with
  `FISSION_BENCHMARK_NO_CACHE=1` on a release `fission_cli` built from `main`
  and from the change, must give byte-identical per-function dumps. This is the
  method used for the `CircleRange` change. The gate needs the eval kit under
  `vendor/decbench-evalkit/`.
* `cargo check --workspace`, `cargo nextest run` for the affected crates (both
  the dev and test profiles), `cargo fmt --all --check`.
* **Effect re-measured, not assumed:** rerun the `--timings` build and the
  commit replay with the new crate graph and report the actual numbers next to
  the model's. No saving is claimed before that.
* Update [ADR 0012](../adr/0012-midend-rename-and-crate-extraction.md),
  `docs/PROJECT_MAP.md`, and the child `AGENTS.md` files that name
  `crates/fission-pcode/src/...` paths.

## Open questions

* Are `infer_entry_register_param_arity` and `seed_nir_render_options` the only
  `midend` items `fission-static` needs? The import scan says yes for `src/` and
  `tests/`; `cargo check` after the switch is the proof.
* Does `midend/cfg.rs` have users inside `midend` that would need the leaf crate
  as a new dependency edge? (`builder` uses it ×11; that edge goes `pcode` →
  leaf, which is the intended direction.)
* Is a smaller `lib-test` achievable without moving tests? The 30k lines of
  in-crate tests are why `fission-pcode`'s test unit is 1.6× its library.
