# ABI call-result bindings preserve observed widths

## Baseline row anchors

The standalone current DecBench eval kit was regenerated from main and scored
with cache-disabled DecBench metrics. All 250 targets were supplied; GED had
240 measurable rows, types 228, and Linux GCC recompilation 158. Union is
72/250, versus the published Kuna snapshot's 88/250. These are local results,
not an official leaderboard update.

| Row | Binary/address | Observed defect | GED / types / byte match |
| --- | --- | --- | --- |
| diffutils, O0, sdiff, flush_line | bin_105.elf, 0x52d6 | DWORD store from a call result becomes a 64-bit local; an unrelated pointer result contaminates the shared return binding | 8 / 0 / 0.4054 |
| gzip, O0, gzip, get_method | bin_219.elf, 0x72f1 | integer comparison calls are cast to pointer-valued shared RAX | 45 / 0 / 0.1301 |
| coreutils, O2-noinline, comm, compare_files | bin_183.elf, 0x2c30 | integer comparison and stream-error results share a pointer-typed RAX | unavailable source CFG / 0 / 0.1228 |

The first anchor's assembly calls getchar_unlocked at 0x52e5, then stores
EAX to [RBP-4]. After its loop, a full-width write reloads RAX from stdin.
There is no need to preserve the first call's upper 32 bits. In contrast,
the later gettext result is consumed at pointer width. The source variable
`c` is currently matched by stack offset but typed as unsigned long long.
Semantic cases for these upstream functions are unavailable, so no source
behavior success is inferred from the scores.

DWARF declaration anchors are `sdiff.c:851`, `gzip.c:1496`, and `comm.c:255`,
respectively. The canonical source locations are retained in the external
DecBench compiled binaries; the stripped eval-kit binaries above are the
decompiler inputs.

## Owner proof

`builder/materialize/call_results.rs::call_result_register_used_by_op`
identifies an aliased ABI return register but returns the ABI carrier's full
width, ignoring the actual operand view. `ensure_call_result_binding` then
creates one hardware-name binding of that width. On x64 a low 32-bit consumer
therefore primes RAX rather than EAX. Later pointer-type constraints can attach
to that same binding. The first incorrect width is created in the builder,
before normalize/type propagation or rendering.

The first narrow-width experiment corrected the call expression's return
width but did not fix the binding: the register namer deliberately canonicalizes
subregister views to one hardware name. Preserve that architectural policy.
A proven narrow call value therefore needs its own typed call-result binding,
resolved by the existing call-site/reaching-definition map, rather than the
shared hardware binding. Live epilogue carriers still use the existing name.

## Invariant and scope

An unknown call result may use a narrower low-prefix binding only when all
reachable uses before a full carrier overwrite or ordinary ABI call clobber
consume that prefix. A later wide use, partial overwrite, unresolved control
flow, or live function return retains the full carrier. Float carriers retain
their existing policy. Calls sharing an argument/return carrier reject
narrowing because their implicit reads may be absent from raw operands.
The existing `builder::init::op_is_lifted_return` fact also rejects CALL/RETURN
pairs, which leave the function rather than clobbering its live result.

The first admitted observation must be a same-width COPY into independent
unique storage. Copies into another hardware register retain the existing ABI
argument and width-conversion policy. A separate call-site binding must dominate
every reachable consumer block; alternate definitions at a nondominated join
need the existing shared carrier. Traverse the original heritage CFG, and
decline narrowing when the structuring dominance graph pruned irreducible
edges. These admission boundaries preserve values before attempting a narrower
representation.

The rule depends on p-code operands, register storage aliasing, CFG successors,
and ABI return-carrier data. No ISA enum, mnemonic, symbol, or corpus identity
is a production guard. It does not infer signedness or an exact source type.
It does not modify the number/order of calls or replace a pointer contract.

This is better independent of the metric: a proven 32-bit result cannot be
mistaken for the unrelated pointer value subsequently carried by the wider
register. Wider live values must remain represented in full.

## Existing owner and risk

Extend call-result observation in the existing ABI owner. Calls have implicit
carrier writes rather than ordinary p-code output definitions, so the existing
CFG observation helpers are the correct place for this proof. No new pass,
dependency, telemetry contract, or owner edge is needed. Bound the CFG walk;
insufficient proof preserves existing behavior.

## Validation matrix

- Synthetic invariant cases: low-prefix-only use; later wide use; distinct
  successor views; partial overwrite; live return; intervening direct/indirect
  lifted CALL/RETURN; shared implicit argument carrier; floating carrier;
  irreducible structuring graph; nondominated join.
- Targeted nextest, then full fission-pcode nextest and pcode/decompiler checks.
- Fresh release CLI generation of the same 250 targets; rerun all three
  standalone DecBench metrics with caches disabled and explicit missing rows.
- External local Docker benchmark baseline/candidate with caches/checkpoint
  reuse disabled. Preserve and report existing compile failures.
- Check previously perfect rows for regressions. Use independent synthetic
  invariants and an unscored corpus subset for regression evidence.
- Commit from main and stage only intended files after measurements pass.

## AI prompt firewall

No other model or subagent was asked for advice. The implementation rule uses
only register byte ranges and CFG reachability; row identity is evidence only.
Do not change output solely to obtain fewer CFG nodes or a higher score.

## Results

Final local measurement (2026-10-02), with fresh CLI output for all 250 targets
and cache-disabled standalone DecBench metrics:

| Signal | Fresh baseline | Candidate |
| --- | ---: | ---: |
| Union | 72/250 | 73/250 |
| GED perfect | 65/240 | 65/240 |
| Types perfect | 15/228 | 16/228 |
| Byte match perfect | 1/158 | 1/158 |
| Recompiles under Linux GCC | 124/158 | 124/158 |

All metric overlays explicitly supply 250 identities, including nulls for
unmeasurable rows; no published Fission score is retained. Relative to the
frozen 2026-09-23 public snapshot, the candidate projects to rank 8. Kuna is
88/250; exceeding it requires 89/250. This has not reached that target and is
not an official leaderboard update.

`flush_line` gains type perfection: its stack local becomes a 32-bit integer,
not a 64-bit value contaminated by a later pointer. Its type score moves 0→1;
byte match moves 0.4054→0.4286 and GED remains 8. The function still has an
incorrect inferred pointer return, so this is a measured width/type recovery
gain, not a claim of whole-function correctness. `get_method` and
`compare_files` remain unchanged under the conservative admission rule.

No previously perfect metric or Union row is lost. Mean type accuracy moves
0.256817→0.261269; mean byte match moves 0.204599→0.204823. Two nonperfect
byte-match rows decrease: `dopass` 0.146179→0.145425 and `argv_split`
0.243169→0.235149. Their compile status and GED stay unchanged. This small
assembly-similarity tradeoff is recorded rather than called regression-free.
All 240 measurable GED values remain unchanged.

Final source patch SHA256 is
`b9152ce3e9b0a128590723b04aded09cca6fb812d5e3b0998eb6bf629041fe4c`;
native CLI SHA256 is
`e13940b77821eeb385c48fe475dec270c4453e9c734e75272177184115556b83`.
Fresh package inputs (223 C files plus variable metadata) are byte-identical
to the checkpoint consumed by the fresh metric runs. The explicit input hash
certificate is preserved with the artifacts; timing telemetry is not a score
input. Generation covers 223 binaries / 250 functions with zero errors.

Validation: seven targeted tests pass; full fission-pcode nextest passes 1,168
tests with one existing skip; pcode/decompiler checks, release build, formatting,
and diff checks pass. The Linux Docker bundle has source fingerprint
`6eb6df2d4bdb314ca1b4cd5698960e17678fab144ba7240b57b5ccdd1214f4df`.
The three motivating rows were also observed through the local Docker service.

External cache-disabled regression evidence: a frozen unscored scale pool of
32 functions across eight programs has zero overlap with the sample-set.
All measured GED/type/recompilation and failure-category values are unchanged;
the same three adapter errors remain. Available pool variants are O0 only,
so this is not optimized-corpus coverage. Six dev smoke rows (O0/O1) are also
unchanged, including their existing compilation errors. The scale pool has no
executable semantic oracle; no new behavioral success is claimed.

Evidence is preserved locally under
`benchmark/artifacts/decbench/2026-10-02-call-result-width/` (gitignored).

Early full-sample experiments admitted arbitrary low-width reads. They raised
types from 15 to 16 perfect rows and Union from 72 to 73, but lowered two
nonperfect type rows and changed two CFGs. Inspection found an alternate
carrier-definition join losing a call result, so the broad admission was
rejected. Keeping a full-width call expression beside the narrow binding also
failed the existing Win64 live-result argument regression (the truncation
could disappear when the binding was inlined). The final candidate preserves
hardware argument copies and nondominated joins, and keeps the admitted call
value and its binding at the same proven width. A trial rejecting all future
calls was discarded because ordinary calls are valid carrier clobbers; lifted
CALL/RETURN pairs now reuse the existing builder fact instead. The final
measurement and external Docker comparisons above apply to the admitted rule.
