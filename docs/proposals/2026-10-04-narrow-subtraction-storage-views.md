# Narrow subtraction reads integer storage views

## Measured anchor and owner

Accepted baseline main: `6ecd26d954274ab3cbd3f1a4fbf22bf59d96d427`.
Union 75/250, GED 65/240, type perfect 18/228, byte perfect 1/166,
recompiles 133/166. Baseline CLI SHA256
`dbbdf1d2ec469d810d6b04abc8372f83f4a1381f4992388f70aa98707d06a6d8`.

Raw real-binary narrow `IntSub` occurs in bzip2 O2 fallbackSort
(bin_048.elf,0xf510:69), coreutils O2-noinline touch main
(bin_009.elf,0x3920:16), and libexpat O2-noinline xcscmp
(bin_020.elf,0x3350:2). Three programs and mixed configurations; these are
structural coverage, not three equivalent compilation failures.

The discarded loop-reservation candidate exposed `uVar1266 -= r11` in
fallbackSort, an integer-minus-pointer compile error. Its raw op reads two
4-byte integer storage slices. Independent baseline login
(shadow O2-noinline,bin_216.elf,0x3d20) lowers `getppid`-derived 4-byte
subtraction through pointer arithmetic `(uint8_t *)(rax) - 1`. Narrow
integer operations should read the integer storage slice, rather than
inherit the full carrier's pointer type. PreHIR contains the defective
interpretation: builder expr/op_lowering is the owner, not the printer
or benchmark fixup. Evidence: primary checkout artifacts
`benchmark/artifacts/decbench/2026-10-04-loop-entry-values` and
`/private/tmp/fission-loop-entry-baseline`.

## Invariant and implementation

A subtraction narrower than the ABI pointer width consumes integer views
of its input storage, even if the shared full-width carrier is pointer typed.
The existing `coerce_integer_storage_view` encodes this at each operand read
and leaves already-correct integer operands alone. Full-width address
arithmetic retains its existing rules. No new pass/helper/dependency,
register-name, function, address or compiler guard.

Without naming a metric: pointer element scaling and pointer provenance
must not define the result of a machine integer word subtraction.
No effects are duplicated or reordered. The independent loop reservation
candidate was rejected for additional uninitialized join consumers; its
production code and three tests are absent from this delivery patch.

## Validation and risks

- Width 1/4 reads of pointer-typed full-width carrier unit coverage.
- Existing signed reassembly and comparison snapshot tests, all pcode tests,
  check pcode/decompiler, strict clippy, fmt, owner-boundary audit.
- Fresh 250-function output/metadata generation; repeat determinism and all
  DecBench metrics with fresh checkpoints/no DB/no metric cache.
- Paired external Docker dev6/fixed unscored32 go/stop regression evidence.
  Do not tune on the pool or promote local results to public rankings.
- Real anchor NIR/HIR/PreHIR before/after and integer-word execution probe.

Risks: signed wrap interpretation, inferred pointer declarations, eventual
type recovery, unknown carrier types, ABI widths. Unknown types provide no
evidence and remain unchanged. No claim that this repairs all integer or
pointer operations. The synthetic invariant test complements real anchors.
No external AI implementation advice or new oracle is used.

## Results

Fresh independent 250 rows generated across 223 binaries with zero errors.
NIR/HIR/variable metadata/assembly are identical across a second fresh run.
22 NIR outputs changed. Measured examples: touch integer-coded option dispatch
reads a 32-bit view before subtracting 97; login reads a 32-bit view before
subtracting 1 from the getppid-derived carrier; fallbackSort casts narrow
operands and recovers fourth argument as `int` instead of `void*`, consistent
with original source `Int32 nblock`. These repeat across three programs and
O2/O2-noinline. Existing measured O0 rows also reproduce the pointer-view
defect: gzip get_method (bin_219.elf,0x72f1) subtracts 1 from the word counter
through pointer arithmetic, and zlib deflate_rle (bin_155.elf,0xa2e6) subtracts
3 from a word read through a pointer-typed carrier. The same unchanged rule
recovers integer reads there, covering O0 as well as optimized code. String comparison anchor has no independent change; do not
claim its unresolved loop identity defect was fixed.

All 1,936 pcode/structuring/normalize tests passed, one existing skip. Checks,
strict pcode clippy, fmt and boundary scan passed. Extracted real 32-bit
subtraction expression passed 16/16 carrier-bit cases under native Clang and
Docker Linux GCC; this is expression coverage, not whole-function execution.

External paired dev6: outputs unchanged, bare compilation 4/6 unchanged,
all wrappers still fail compilation. Fixed unscored32: one output changed,
bare compilation 8/32 unchanged; 28 rows have no wrapper and 4 have preexisting
adapter errors. Contracts unchanged, no recovered checkpoint rows and no new
status failures. Pool is go/stop evidence only; no whole-corpus parity claim.

Fresh no-cache DecBench evaluation retained Union 75/250 (30.0%), with zero
perfect-function gains/losses. GED 65/240 and mean 33.675 are unchanged.
Types 18/228 perfect retained; fallbackSort score improved from 5/49
(0.1020408163) to 6/49 (0.1224489796). Mean type score increased from
0.2656858953 to 0.2657754048. Byte perfect 1/166 and recompiles 133/166
are unchanged; mean byte score decreased slightly, from 0.2119106337
to 0.2118900452. This tradeoff is disclosed: explicit integer word reads
and original-source integer argument recovery justify the change independently
of assembly-distance noise. Do not claim a byte-match gain or whole-function
parity.

Fixed91 retained Union 23/91 and recompiles 67/91. Against frozen 2026-09-23
rivals this is local overall rank 8, fixed91 tied rank 5. Public rankings have
not been updated; exceeding frozen Kuna 88/250 still requires 14 additional
perfect functions. The primary benefit measured here is one additional matched
variable type, not a new perfect function. #182 remains open for incoming
value lifetimes and loop/predecessor-copy reservation work.
