# FID/GDT in Fission — how it works, and where it's tested against real Ghidra

*Scope: everything below is validated against **Ghidra 12.0.4** specifically. The on-disk formats this
depends on — the Java-serialization/streaming-ZIP wrapper, the 128-byte XOR mask on chained buffers,
the `LocalBufferFile` B-tree node layout — are Ghidra's internal database format, not a documented or
versioned interchange format, so none of this is guaranteed to hold on older or newer Ghidra releases.
It hasn't been checked against any other version.*

*Corpus: 57 `.fidb`/`.fidbf` databases, covering:*
- *Architectures: x86 (32/64), AArch64, ARM (32), MIPS (BE/LE, 32/64), PowerPC (BE, 32/64 A2ALT), SuperH4,
  68000/ColdFire, PA-RISC, SPARC, AVR8.*
- *Toolchains/libraries: GCC+glibc per architecture above, RHEL/CentOS el6/el7 (i686/x86_64), MSVC
  (VS2012/2015/2017/2019/vsOlder, x86/x64), plus vendor-specific families (Qt5, OpenSSL 1.0.1u/1.0.2l/1.1.0f,
  libsodium, SDL, teskalabs).*

The core design split: FID gets a full native reader + hasher because it's matched at decompile time
against those 57 databases depending on detected compiler/language — that needs to be fast and robust.
GDT doesn't need any of that: it's a small, fixed, known-ahead-of-time set of archives, so it's extracted
**offline once** by a Python script into plain JSON/packed tables, and there is no runtime `.gdt` parser
in Fission at all. Different problems, different amount of investment.

## FID — three layers

### 1. Raw reader

`.fidb`/`.fidbf` isn't one format:
- The legacy SQLite variant — detected, explicitly unsupported.
- The real one: a Java-serialization-wrapped, streaming-mode ZIP entry (size in the trailing data
  descriptor, not the local header) whose payload DEFLATE-decompresses to Ghidra's actual
  `LocalBufferFile` — a B-tree of long-key interior/var-record/fixed-record leaf nodes, with large
  ("chained") records XOR-obfuscated against a fixed 128-byte mask.

All of that's ported natively — no JVM, no SQLite dependency.

**Reproduce:** two of the 57 databases (`gcc-MIPS.BE.32.default`, `gcc-avr8.LE.16.extended`) have an
unallocated root buffer for their (empty) relation table — Ghidra's own `root_buffer_id = -1` convention
for "first buffer not yet allocated." Reading that as a real buffer id rejected both files outright.
Pinned by `a_database_with_no_relations_parses` — `cargo test -p fission-signatures a_database_with_no_relations_parses`
— which parses both stems directly and asserts they no longer error.

### 2. Repacked storage

None of the B-tree structure earns its keep once read — Fission only ever reads a `.fidb` whole, never
writes one — so everything gets flattened into a custom `.fpk`: sorted record tables, zstd + columnar,
sparse-indexed, mmap'd.

- **254MB → ~65MB** (functions pack 4.9x, relations 2.5x), every field preserved — including the relation
  table, since `force_relation` depends on it.
- A `LazyFidDatabase` answers `identify_by_hashes` straight from the on-disk hash index without
  materializing or indexing the table — the eager `.fidbf` parse-then-index path costs **~65ms per
  database** before the first query, which is exactly your cold-start concern.

**Reproduce:** `cargo test -p fission-signatures packed_tables_answer_what_the_source_database_answers`
parses a source `.fidbf` eagerly, opens the same database's `.fpk` lazily, and asserts both answer
`identify_by_hashes` identically over a stratified 200-function sample per database (requires
`utils/source/fid/` present locally — the raw databases aren't committed, only the packed output is).

### 3. The hasher

This is the part that actually matters for correctness. `MessageDigestFidHasher.hash()` is ported
bit-for-bit into Fission's SLEIGH runtime — same FNV-1a64 constants, same `(op_index+1)*7777` /
`0xfeeddead` / `1234567*67999` mixing arithmetic, same masked-byte digest. The interesting bugs were all
in *operand classification*, not the digest itself:

- Ghidra's `getOpObjects`/`getOpsPrintOrder` only counts display operands *after the first whitespace* —
  x86 prints `CMOV^cc`'s condition code and `CMPSB^rep`'s prefix *inside* the mnemonic, so Ghidra doesn't
  count them as operands at all. Counting them shifted every later operand's index and fed the condition
  code's own unique-space temp into hashing, which matches no known shape — so the whole function's hash
  was refused. **On a statically-linked glibc this alone accounted for 64% of functions.**
  **Reproduce:** `cargo test -p fission-sleigh an_operand_printed_inside_the_mnemonic_is_not_an_operand`
  — decodes `cmovnz rax,rdx` and `setnz al` and asserts the display operand count (2 and 1) matches what
  Ghidra's `Constructor.getOpsPrintOrder` reports, not the naive "everything with a handle" count (3 and 2).

- Distinguishing "this handle is an address" (placeholder in the specific hash) from "this is a real
  scalar" isn't `OperandType` at all once you're outside Ghidra's own AST — the signal that actually
  matches is `RuntimeFixedHandle::space == "ram"`: true for a dereferenced load *and*, surprisingly, for a
  direct `CALL`/`JMP` target too (both resolve through the code/ram address space), false for `LEA`
  (computes a value) and plain immediates.
  **Reproduce:** `cargo test -p fission-sleigh fid_full_hash_matches_ghidra_exactly_for_rip_relative_memory_load
  fid_full_hash_matches_ghidra_exactly_for_rip_relative_lea` — two GCC-assembled instruction sequences at
  real addresses (`0x401702`/`0x40170d`), each asserting the exact `full_hash` u64 printed by a headless
  Ghidra script running `FidService.hashFunction` on the equivalent real instructions
  (`3768fc2909545fcc` / `ae465fd70004f692`). The classification rule itself (not just these two cases) was
  derived by printing `Instruction.getOperandType(ii)` / `OperandType.isScalar` / `isAddress` directly from
  a headless script over six hand-picked instruction shapes — see
  `fid_hashes_match_ghidra_exactly_for_specific_hash_operand_classification` for the full enumeration.

- SIB addressing always contributes a `Scalar` object for the scale factor even when `scale == 1`, and
  omits displacement entirely when `disp == 0`.
  **Reproduce:** `cargo test -p fission-sleigh fid_full_hash_matches_ghidra_exactly_for_sib_addressing` —
  three GCC-assembled SIB variants (`[rax+rcx*4+0x10]`, `[rax+rcx*1]` with disp=0/scale=1,
  `[rax+rcx*8+0x100]` with a 32-bit displacement encoding), each checked against a headless script's
  `Instruction.getOpObjects(ii)` printout *and* `FidService.hashFunction`'s exact `full_hash`.

**A concrete false positive**, found by scanning the real `vs2012_x64` database for one full_hash shared
by differently-named functions with `auto_fail=true`:

```
full_hash=0x73d3b025f0122566, code_unit_size=3, auto_fail=true, shared by:
  ??1Image@Gdiplus@@UEAA@XZ   → Gdiplus::Image::~Image
  ??1Bitmap@Gdiplus@@UEAA@XZ  → Gdiplus::Bitmap::~Bitmap
  ??1CByteArray@@UEAA@XZ, ??1CDWordArray@@UEAA@XZ, ??1CObArray@@UEAA@XZ,
  ??1CPtrArray@@UEAA@XZ, ??1CUIntArray@@UEAA@XZ, ??1CWordArray@@UEAA@XZ,
  ??1CGestureConfig@@UEAA@XZ,
  ??1_Concurrent_queue_base_v4@details@Concurrency@@MEAA@XZ
```

Ten unrelated virtual destructors (GDI+, MFC array classes, a Concurrency Runtime internal, an unrelated
gesture-config class) all compile to the same 3-byte thunk, so they all hash identically. Ghidra's own
database builder detected exactly this and flagged the hash `auto_fail` — "never return a match for this,
regardless of context." Before this flag was honored, matching this hash against a real binary would
report *one arbitrary name from this list of ten* with no way to tell which. This one hash alone accounts
for 10 of the corpus's 38,465 auto_fail (2.10% of ~1.83M) functions; the aggregate number is real, not an
artifact of a handful of degenerate cases — but this is the shape the flag exists for.

`auto_pass` runs the opposite direction: it waives the code-unit-size floor for a hash Ghidra's builder
has independently verified as safe despite being small (all 156 auto-pass functions in the corpus score
below the normal accept threshold on size alone) — before this was honored, every one of them was being
silently dropped instead of returned.

**Reproduce (flag semantics):** `cargo test -p fission-signatures auto_fail_is_never_returned
auto_pass_is_returned_below_the_size_threshold auto_fail_beats_auto_pass` in `fidbf/types.rs` — synthetic
fixtures pinned directly against the flag semantics stated in Ghidra's own `building_fid.txt`.

Net effect: on one statically-linked test binary, fixing the operand-counting bug alone took
identifications from **1 to 113**.

FID is correctly inert against DecBench-style binaries, for what it's worth — those are dynamically
linked, so there's no statically-embedded library code to match against at all. Not an implementation
gap, just what the corpus is.

## GDT

Much less interesting by comparison, which is sort of the point. The struct/enum/function-signature
records live in the exact same `LocalBufferFile` B-tree format as FID (composite/component/pointer/
array/enum, keyed by a 64-bit Data-Type-ID whose top byte is `DataTypeManagerDB`'s kind tag), so it's the
same format knowledge, but there's no reason to carry a runtime reader for ~10 archives that never change
per-run. A one-shot Python script (`gdt_extract_structs.py` / `_signatures.py` / `_enums.py`) walks the
tree once and emits `structures.json` / `base_types.json` / packed `.fpk` signature tables; Rust just
loads those. **36MB** of source `.gdt` (windows_vs12 32/64, mac_osx, rust-common, generic_clib 32/64,
golang 1.15–1.25) compresses to about **8.5MB** of derived data, and that's what feeds type/prototype
propagation. This side is untested against multiple Ghidra versions at all — it was only ever run against
whatever GDT files shipped with the Ghidra install used to source them, so treat the format notes below
as similarly 12.0.4-scoped.

## Implications for Kuna

- A native FID *reader* is worth building only if matching runs often enough, against enough different
  database families per session, that the ~65ms eager-parse cold start per database actually adds up —
  which is exactly angr/Kuna's shape (arbitrary target, unknown toolchain, per-session cost matters) more
  than a one-off analysis tool's.
- The *hasher* is the harder half and the one that actually determines match quality — the format work
  is finite effort, but silently under-hashing (the CMOVcc/SETcc bug: 64% of one binary) or over-trusting
  a match (the auto_fail gap) both fail quietly, with no signal that anything is wrong. If Kuna ever
  reimplements this, budget for cross-checking against a real headless Ghidra script from day one, not as
  a later pass — every bug above was found that way, none by code review.
- GDT: offline extraction is clearly sufficient for Kuna too, *unless* Kuna wants to accept arbitrary
  user-supplied `.gdt` files at runtime rather than a fixed bundled set — in which case the raw B-tree
  reader is the reusable 80%, since it's the same format as FID.
- On the actual "accessible to agents" problem you raised: the fix isn't a scripting/MCP wrapper around
  Ghidra's own Java FID service — that's the JVM-dependency, subprocess-round-trip, serialization-boundary
  shape you're already avoiding. It's exposing `identify(addr) -> [{name, score}]` as a plain function at
  the *same* layer the agent already queries (function list, disassembly) — no new protocol, no new tool
  surface, nothing for the agent to discover or learn to call differently.

## Distribution boundary — worth being deliberate about

Two genuinely different postures, and it's worth stating outright which one this is, rather than leaving
it ambiguous:

1. **Repack-and-redistribute** (what Fission currently does): the `.fpk` files shipped are a lossless,
   re-encoded copy of data Ghidra itself bundles and distributes under Apache 2.0. That license permits
   redistribution of derived works, but it obligates carrying attribution/NOTICE and stating what was
   changed — I haven't verified Fission's own NOTICE/attribution file actually says this correctly, and
   I'm not the right authority to certify it does; that's worth an explicit compliance pass rather than
   an assumption. This is a live question, not a settled one.
2. **Local-extraction-only**: ship the reader/converter code, not the converted data — the user points it
   at their own Ghidra install's FID/GDT files and the extraction happens on their machine. This sidesteps
   the redistribution question entirely (nothing of Ghidra's ever leaves the user's machine through your
   project), at the cost of requiring a Ghidra install to exist locally.

I don't know the specifics of the licensing/attribution issue that came up in this server before, so I
can't say whether it maps directly onto this — flagging the two postures so that's an informed choice
rather than an implicit one, on both sides.
