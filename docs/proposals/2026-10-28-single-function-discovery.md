# One function discovery

Base: main `2bd9720a9`. This is an inventory and consistency change. It makes no
decompiler-quality claim; the DecBench section only checks that decompilation did
not move.

## 1. What was split

Function finding existed as three profiles of one analyzer, selected per command:

| Where | Profile before |
| --- | --- |
| `decomp`, `list`, `info`, `callgraph`, `xrefs`, `strings --context`, ... | `conservative` by clap default |
| other subcommands | none: no discovery ran |
| `db`, the script runner | `balanced`, hard-coded |
| `--emit-program-metadata` | `conservative`, run a second time on a clone |

`--function-discovery-profile` was copied into eleven argument structs and read back
through about thirty assignments. The docs told users to "use the same profile in
subsequent `callgraph`, `list` or `xrefs` queries to keep discovered entries aligned",
and a unit test asserted that every command resolved functions the way `list` did. The
inconsistency was known and was managed by plumbing.

## 2. Measurement that chose the algorithm

224 stripped kit binaries (`fission_cli list --json`, no network, uncached). Truth for the
74 O0 binaries is every source function the sweep scores (21,991 addresses); for the 150
O2 / O2-noinline binaries only the 154 benchmark targets are known.

| Profile | Recall, O0 all-source | Recall, benchmark targets | Addresses found |
| --- | --- | --- | --- |
| none = `conservative` (the old default) | 11,549 / 21,991 = **52.5%** | 131 / 154 = 85.1% | 67,769 |
| `balanced` | 21,035 / 21,991 = **95.7%** | 144 / 154 = 93.5% | 123,168 |
| `aggressive` | 21,035 / 21,991 = 95.7% | 144 / 154 = 93.5% | 123,174 |

`conservative` and "none" are identical, so the old default found barely half of the
functions a user could ask for. `aggressive` adds six addresses over `balanced` on 224
binaries, none a benchmark target. Cost of `balanced` over `conservative`: median
+0.07 s, p95 +0.6 s, worst case +5 s (u-boot).

Decision: keep the `balanced` stages, delete the `aggressive`-only prologue-fingerprint
scanner, and run one discovery for every command.

## 3. What the first attempt showed

Running `balanced` for `decomp` as well (all the profile flag needs) changed the
decompiler's output on 16 of 223 functions and made structure distance worse
(mean GED 33.34 -> 36.45, 4 rows worse, 1 better, no perfect lost):

```text
sub_8001714   10,948 chars -> 47 chars   (body gone)
sub_8028164  124,670 chars -> 4,688 chars
```

Cause: for a function of unknown size the decode window is the distance to the next
function start, and a branch to a known start becomes a tail call. `balanced` finds a
start four bytes into `sub_8001714` (`0x8001718`). A start that only a data-reference
or signature scan names can land inside a function, and the decompiler believed it.

## 4. Invariant

A function start that only a reference or signature scan names belongs in the
inventory. It is not evidence that the function before it ends there, and it is not
a function the decompiler should tail-call into. The decompiler reads the table of
functions that something else vouches for: a symbol, an unwind record, debug data, the
load configuration, or a validated direct call.

Implementation:

- Discovery marks scan-only starts with `origin = "sleigh-scan-inferred"`
  (`FunctionInfo::is_scan_inferred`). Starts from data references, the Ghidra pattern
  database and shared-return recovery are scan-only; direct call targets, symbol seeds,
  MSVC EH and load-config hits are not.
- `LoadedBinary::without_scan_inferred_functions()` returns the binary with those
  entries removed. The one-shot decompile path selects functions from the full
  inventory and decompiles against this view.
- `FunctionDiscoveryProfile`, the CLI flag and its eleven plumbing sites, the
  `aggressive` scanner (`scan_dynamic_prologues`, fingerprint helper, two tests) and a
  stray debug write of `/tmp/fission_fp_stats.json` on every balanced run are removed.
  Passing the old flag is now an error, covered by a test.
- `db`, `--emit-program-metadata` and the script runner call the same discovery;
  metadata no longer runs it a second time on a clone.

## 5. Validation

Full Union pipeline (uncached, same patched evaluator, Rust Joern GED), unified build
against main `2bd9720a9` (`runs/main-2bd9720a9-fixname` vs `runs/disc-unified3`):

| | main | unified |
| --- | --- | --- |
| Union / 250 | 76 | 76 |
| GED perfect / measured, mean distance | 66 / 235, 33.472340 | 66 / 235, 33.472340 |
| Type perfect / measured, mean | 16 / 226, 0.233773 | 16 / 226, 0.233773 |
| Byte perfect / measured, mean | 1 / 158, 0.220036 | 1 / 158, 0.220036 |
| Rows whose score changed | - | 0 |
| Perfect matches lost | - | 0 |

Decompiled text changed on 3 of 250 functions (+2 chars, +6 chars, and
`0x800d724` in a stripped Cortex-M image, 9,005 -> 6,030 chars). None moved a score.
The Cortex-M case is expected: the old default found 52 functions in that image
(its linear sweep decodes Thumb out of step) and the unified discovery finds 887, 412
of them direct call targets, so the function now ends at the next real start instead of
running on into its neighbours. Nothing here is claimed as a quality gain.

The first attempt (no scan-only marker) is why the marker exists: before it,
mean GED was 33.34 -> 36.45 with 16 changed functions.

Tests: `cargo nextest run -p fission-static -p fission-loader -p fission-cli
-p fission-script -p fission-core -p fission-project` 543 passed, 2 skipped;
`cargo check --workspace --tests` clean; `cargo fmt --all --check` clean. Two tests
fail without their fix: the flag-rejection test and
`naming_a_scan_inferred_function_keeps_it_in_the_decompiler_view`. The existing
`a_chosen_signature_reaches_the_callers` found the second defect (a name chosen with
`db` was lost for a scan-only function) before it shipped.

Not run: debug-profile suites, `fission-pcode` / `-decompiler` suites (no code in those
crates changed), the DecBench run does not exercise `db` (it passes `--no-db`).

## 6. What this does not do

- Scan precision is unchanged. In one Cortex-M test image 25 of 56 discovered starts are
  scan-only, and the earlier example shows a scan can land inside a function. The
  inventory (`list`, `callgraph`, `xrefs`, `decomp --all`) still contains them.
  `xref_index` and `callgraph` attribute addresses to the nearest start, so a scan-only
  start inside a function still takes the tail of that function in those two views.
- `decomp --all` now selects from the larger inventory, so it decompiles scan-only
  starts too.
- The other dead scanners (`scan_jmp_thunks`, `scan_cc_padding_regions`, kept behind
  commented-out call sites) were left alone.

## Re-measured on `main` b6df9aa79 (2026-11-02)

Rebased onto a `main` that has the per-function ISA mode (#722, #723). Base is that
`main` with its default (`conservative`) discovery; the change is the single discovery.
Same addresses decompiled by both builds (taken from the base build's `list`).

| What | Result |
| --- | --- |
| Function table, default | 490 -> 2,406 on `bin_004` (the old `balanced` count, exactly) |
| DecBench sample-set (gate, 233 rows) | perfect 68 -> 68, total distance 7,769 -> 7,731; one row differs (`mspProcessInCommand`, 472 -> 434) |
| Decompile output, 4,170 ARM functions (157 files) | 46 differ (1.1%) |
| Decompile output, 708 x86-64 PE functions (91 files) | 3 differ; the one read in full is a callee named `_set_new_mode` instead of `sub_140001e52` |

The decode window of a function of unknown size is the distance to the next table
entry. With ELF symbols as ground truth (99 ARM files, symbols stripped; entries tagged
scan-inferred left out, as the decompiler sees the table):

| | True functions in the table | Window runs past the real size | Window cuts into the function |
| --- | ---: | ---: | ---: |
| `conservative` (before) | 4,822 | 1,745 (36.2%) | 3 (0.1%) |
| single discovery (after) | 34,891 | 4,327 (12.4%) | 116 (0.3%) |

So the output moves because the table is right more often: the old table's window
swallowed neighbouring functions. `bin_004` `0x8004ce4` went from 3,109 to 1,338
characters and its real extent is `0x8004d58 - 0x8004ce4`. The price is 116 entries that
sit inside a true function, which are call targets found by the linear sweep after it
resynchronises past data (the old `conservative` sweep abandoned the rest of a chunk at
the first undecodable word, which is also why it found fewer). They are not tagged
scan-inferred because direct calls are evidence the decompiler already trusts.

The claim above ("decompilation did not move") therefore does not hold byte for byte. On
the 233 rows the gate scores the perfect count is unchanged and the total distance is
not worse (one row moved, 472 -> 434); Union was not re-run on this base. On ARM firmware
about one function in a hundred prints differently.

