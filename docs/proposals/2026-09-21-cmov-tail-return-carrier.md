# Proposal: Keep guarded return-register writes out of unconditional merge bindings

## Motivating measured row

- Real corpus: DecBench `dev`, `win32_status_gcc_O2.exe`
- Function: `describe_error`, address `0x140001530`
- Source: `source/c/win32_status.c`
- Baseline command:

  ```text
  FISSION_ENDPOINT=http://localhost:8000 FISSION_BENCHMARK_NO_CACHE=1 \
    .venv/bin/python runner/runner.py --corpus dev --function describe_error \
    --decompilers fission --run-mode local --no-resume \
    --output results/issue107_before_4688dfcf0.json
  ```

- Baseline result: 7 compiler variants; `gcc -O1`, `-Os`, and `-O3` fail
  semantic assertions, while `gcc -O0` and `gcc-m32 -O0` pass. The direct
  `gcc -O2` decompilation contains an uninitialized `xVar12` return and omits
  the default `"other"` result.

## Observed semantic shape

The lifted x86-64 p-code represents a conditional move as an instruction-local
conditional branch followed by a guarded write to the ABI primary return
register:

```text
RAX = default_value
CBranch skip_to_return, predicate
RAX = conditional_value
return
```

The conditional branch is intentionally not a CFG terminator. Its guarded copy
is therefore not an unconditional definition of the successor's live-in value:
one path reaches the return without executing that copy.

## Owner proof

Builder diagnostics show that the guarded `Copy RAX <- ...` is initially
eligible for the generic primary-return live-out proof. Before that choice is
used, `merge_binding_name_for_direct_successor_accumulator()` sees the return
join as a multi-predecessor accumulator and creates a fresh explicit merge
binding. That binding wins the materialization precedence, so the guarded copy
is emitted as `xVar12` while the join's return recovery reads `xVar12` on both
paths. The skip path consequently returns an uninitialized binding.

The defect is in the generic materialization proof ordering and its missing
conditional-definition exclusion. It is not in string recovery, return
printing, the function address, or the compiler-specific binary.

## Invariant and proposed change

The direct-successor accumulator proof may claim a definition only when the
definition executes on every path from the current block to the successor
entry. A definition strictly inside an instruction-local forward-CBranch body
does not satisfy that condition and must be left to the CMOV/register-carrier
materialization rules.

Add this proof guard to the canonical direct-successor accumulator owner. The
proof must apply it both to the definition currently being considered and to
the selected last definition on every other incoming predecessor; one guarded
incoming edge is enough to invalidate the unconditional merge candidate:

```text
if any selected incoming definition is inside an instruction-local
forward-CBranch body:
    decline unconditional successor merge
```

No architecture, function, address, string, or compiler guard is needed. The
existing CFG-local CMOV classifier supplies the control-flow fact, and the
existing primary-return live-out rule then keeps the ABI carrier stable.

## Regression and validation matrix

Add a synthetic p-code regression with a default primary-return write, a
forward instruction-local CBranch, a guarded primary-return write, and a
multi-predecessor return join. Assert that the guarded write uses the stable
primary-return carrier and that the join does not return a binding initialized
only by the guarded body. Keep the existing saturating-CMOV regression green.

Re-run the exact DecBench row with caches disabled and compare the same seven
variants. Then run the materialization/pcode tests, the emulator suite, format
and diff checks, and a release build.

## Risks and non-goals

The guard must not disable explicit merge bindings for ordinary loop-carried or
unconditional register updates. It only rejects definitions proven to lie in a
conditional instruction-local body. This preserves existing accumulator and
loop-carrier behavior while removing an invalid name-precedence path.
