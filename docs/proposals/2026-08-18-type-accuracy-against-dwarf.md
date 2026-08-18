# Type accuracy against DWARF

Measuring DecBench's second metric locally, and what it says to work on.

fission-benchmark ships the original C source and builds with `-g`, so unlike
the DecBench submission corpus its type ground truth is computable here.
`runner/type_match.py` is a port of DecBench's own scorer: arguments match by
ABI position, stack locals by calibrated frame offset, everything else by exact
name.

Every figure below excludes rows that decompile a different function than the
manifest names -- see `2026-08-17-undefined-name-in-output.md` section 9.

## Two scorer bugs came first

The port under-reported Fission by 18 points before any of it measured
decompiler behaviour.

**Pointer spellings never reached `TYPE_MAP`.** The port carried the table but
not upstream's `_map_pointee`, so the table only fired on a whole-string match.
`longlong` normalized and `uchar *` did not, and `uchar *` cannot intersect
DWARF's `char*`. The single largest bucket was 83 arguments where ground truth
accepted both `char*` and `unsigned char*` and Fission wrote `uchar *`. angr
does not move when this is fixed -- it already prints `char*`. Ghidra-style
spellings absorbed the whole penalty.

**32-bit PE decoration defeated the definition parser.** `_find_definition_params`
anchored the function name with `\b`, which cannot straddle the leading
underscore of `_list_sum`, so every gcc-m32 row parsed zero arguments while
ground truth had one to four. Read as decompiler behaviour it looked like
wholesale argument loss.

|  | perfect | mean accuracy |
|---|---|---|
| as ported | 19.7% | 44.0% |
| + pointer spellings | 21.0% | 50.2% |
| + underscore decoration | 29.5% | 62.8% |
| angr, same fixes | 22.7% | 54.9% |

Recovered arity agrees with DWARF on 97.0% of rows.

## Where the remaining loss actually is

Splitting the score by variable kind moved the target completely:

| | tp | fp | fn | accuracy |
|---|---|---|---|---|
| arguments | 628 | 137 | 2 | **81.9%** |
| stack locals | 153 | 65 | 397 | **24.9%** |

The 397 is not a recovery gap. 398 ground-truth locals carry no frame offset --
they live in registers, all of them at `-O1` and above -- and the scorer can
only match those by exact source name. No decompiler recovers `sum` or `i`. On
the 217 locals that do carry an offset, Fission scores 56.2% against angr's
29.0%.

## What was fixed

**Parameters read only through a truncating return.** `int add_ints(int, int)`
at `-O1` came back as `add_ints(longlong, longlong)`.
`narrow_integer_params_from_wrapping_return_uses` exists for exactly this and
walked only the return expression, but the body reaching it is
`rax = (int)(param_1 + param_2); return rax;` -- one intervening assignment hid
both parameters. Following a returned name into its definition, when the body
assigns it once and uses it once, fixed 6 rows and regressed none. Type accuracy
61.46% -> 63.15%; gotos, line count and bare-compile all unchanged.

## What was measured and abandoned

**Stack locals declared wider than the slot.** 19 of the 43 stack-local
mismatches are a 4-byte slot declared `ulonglong` or `longlong` -- `unsigned sum`
accumulating in `[rbp-4]`, returned through RAX, so the return-type constraint
pinned it 64-bit.

A pass narrowing such a local to the width actually stored into it passes
unit tests in both directions and never fires on a real binary. Instrumenting it
shows why: at every one of the seven points the type fixed-point runs, the body
contains no `Assign { lhs: Var("local_4") }` at all. Stack slots are still
written through memory at that stage; the `local_4 = uVar18` form the renderer
shows appears only after normalize. The evidence a width decision needs -- the
store width -- is in the memory representation, not in statement-level
assignments, and reaching it is a different analysis than the one attempted.

The pass was removed rather than left inert.

## The library corpus

Every other C source in the corpus declares each struct it names in the same
translation unit, so a type library has nothing to recognize: of 380 decompiled
functions, not one declares a type the library could have supplied. That makes
library-driven recovery unmeasurable rather than weak.

`corpus/dev/source/c/libc_types.c` (10 functions x 7 gcc variants) takes or
builds types the library does know -- `FILE` through a parameter and a return,
`struct tm` through a pointer, `div_t` by value, `time_t`, `size_t`, `char **`
from `strtol`. DWARF records them under those names, so recovery can be scored.
Fission's baseline there is 23.7% against 63.1% on the self-contained corpus.

Two defects surfaced immediately and are fixed:

**A COFF section symbol shadowed the function at its address.** Calls into the
static C library rendered `.text(param_1, ...)`. A `-g` build keeps the COFF
symbol table, every linked object contributes a `C_STAT` symbol carrying its own
section's name, and after linking that symbol sits on the object's first byte of
the merged section -- the first byte of a real function. `.text` became a name
fact, and facts outrank direct symbols, so `fopen` lost 0x140002c00.

**A signature type of `int` was treated as evidence.** The GDT extractor
recovers parameter names reliably but resolves a type ID only when it lands in
the built-in table; typedefs, composites and pointers fall through to `int`.
30,722 of 31,410 C-library entries and 96,081 of 115,900 Win32 entries are
stored that way, `fopen|int|__filename:int,__modes:int` among them. Applying
that overwrites inference with a placeholder. The test has to be per type
string: judging `difftime|double|_Time1:int,_Time2:int` as a whole keeps the
`double` and takes both placeholders with it.

## The extractor itself

Two defects, one behind the other.

**`resolve_type` never decoded the kind byte.** It looked up a Data Type ID in a
map keyed by the raw table key, but the ID carries its kind in the top byte, so
only built-ins (kind 0) ever matched and every typedef, composite and pointer
fell through to `int`. That is why both signature files were typeless.

**`parse_pointer_table` read the wrong buffers.** It walked every
`LONGKEY_FIXED_REC` buffer and read each as an array of 17-byte pointer records.
`FixedRecNode` is the leaf type for EVERY table with fixed-length records, and
its record length comes from the table's schema rather than from the node, so a
node's own bytes cannot say whether it holds pointer records. In
`generic_clib_64.gdt` that produced 16,199 "pointers" of which 1,404 are real,
and the decoded pointee IDs carried 250-odd distinct kind bytes where only 0-8
exist. Fixing only `resolve_type` therefore made things worse, not better:
`FILE *fopen(const char *, const char *)` resolved to `int*`, which looks like a
recovered type in a way `int` does not.

Three properties identify a genuine pointer leaf, and together they select
exactly the three real ones out of 32 candidates: the key/record array fits the
buffer at a 25-byte stride, leaf keys are strictly ascending, and every key is a
Data Type ID of kind POINTER -- this table's keys are its own IDs, so one
foreign key means a foreign table. Verified against Ghidra 11.4.2's
`FixedRecNode` and `PointerDBAdapterV2`.

With both fixed, the C-library prototypes resolve exactly:

```
fopen|FILE*|__filename:char*,__modes:char*
fgets|char*|__s:char*,__n:int,__stream:FILE*
strlen|size_t|__s:char*
strtol|long|__nptr:char*,__endptr:char**,__base:int
localtime|tm*|__timer:time_t*
difftime|double|__time1:time_t,__time0:time_t
```

Typeless C-library entries fall from 97.8% to 79.6%. Struct extraction improves
with the same table: 510 structs in that archive get corrected field types
(`int*` -> `X509_ALGOR*`, `int*` -> `char*`) and 63 more are recovered.

Measured end to end, regenerating only the two `generic_clib` signature files:

| | before | after |
|---|---|---|
| libc slice type accuracy | 19.14% | **24.86%** |
| self-contained corpus | 63.26% | 63.26% |
| improved / regressed | | 8 / **0** |
| gotos, bare-compile | 36, 308/428 | 36, 308/428 |

Decompiled output now declares `FILE* __stream;` and `char* __filename;` where
it declared `ulonglong`.

### Two things this does not cover

Regenerating `mac_osx` and `windows_vs12_{32,64}` was reverted. Those files went
3,801 -> 46,781 and 8,119 -> 100,983 lines, and since this change cannot affect
how many function definitions are found, the shipped files were not produced by
this version of the script. Replacing them would mix an unrelated content change
into a type fix. Only `generic_clib` and `generic_clib_64` were regenerated,
where the function-name set is identical to what shipped.

`win_api_signatures.txt` is 82.9% typeless too but carries a header comment and
hand-curated entries (`CreateFileA|HANDLE|lpFileName:PSTR,...`), so it is not a
plain GDT extraction and regenerating it would discard that.

The function's own parameters are still untyped -- `open_reader(ulonglong)`
where the local holding `fopen`'s result is now correctly `FILE*`. Propagating a
typed local back to the parameter it was copied from is the next step, and it is
ordinary type flow rather than library work.

## Not winnable on this corpus

Around 40 argument mismatches are named source structs -- `Pair*`, `Kv*`,
`Node*`, `ConfigNode*` -- recovered as `uint *` or `int *`. Those names exist
only in the benchmark's own sources, in no library, and angr scores zero on them
too. They are not a type-library gap; they are outside what the metric can
credit any decompiler for.
