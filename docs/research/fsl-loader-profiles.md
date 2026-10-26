# Owned loader profile selection

Date: 2026-10-19. Initial opt-in implementation; not complete asset migration.
The existing product SLEIGH path remains the default.

```text
canonical detection -> ELF schema facts -> owned catalog selection
  -> existing ELF mapping/relocations/symbols -> canonical LoadedBinary
  -> decoder ID + package hash binding -> existing x86 FIR -> output backend
```

## Ownership and transition

`fission-loader::loader::profiles` owns typed target facts, immutable validated
catalogs and selection errors. Probe and mapping reuse the canonical ELF schema.
The owned route selects a profile before mapping, rechecks its rule during
parsing, creates no Ghidra language ID or `BinaryLoadSpec`, and does not invoke
the high-level external-debug/signature enrichment route.

`ProfiledBinary` wraps the canonical `LoadedBinary` plus its selection inputs and
profile, not a parallel analysis database. Existing serialized payload fields
and legacy entry points remain unchanged. Propagation through the immutable
product analysis snapshot remains future work.

`fission-fsl::profiles` compiles reviewable catalog source into binary records.
The independent `loader` feature enables this module; `x86` also enables
`loader`. No dependency was added. This catalog does not change FIR semantics.

## Source contract

`specs/x86/elf.fslprofiles` supplies two authored rules for the existing bounded
ELF i386/x86-64 package. This is not an imported opinion/ldefs corpus.

```text
profiles 1 {
    profile fission.x86.elf64.scalar {
        match elf machine 62 bits 64 endian little flags 0x0 0x0;
        target x86 scalar;
        decoder owned-x86-v2;
        package 4f21befdc46ff60cb74913f0e2dea88b770cc1edbcb414590c99a72b90a67470;
        abi unselected;
    }
}
```

One declaration per line, fixed declaration order, ASCII identifiers and `#`
comments. Matching requires format/machine/width/endian equality and
`(file_flags & mask) == value`. Mask zero leaves flags unconstrained. Duplicate
IDs and overlapping rules refuse; source order is not priority. An unmatched
binary refuses. Processor/variant labels are authored claims, not independent
proof that a machine number implements that ISA.

`decompile-profiled` requires the selected decoder ID and SHA-256 to match the
actual compiled encoding/FIR package before lifting. `select-profile` only
reports selection; it does not establish package availability or validated
behavior. ABI is explicitly unselected. Compiler/OS ABI evidence, register-layout
resolution, calling conventions, fixups and injections are not implemented here.

## Binary contract

`.fslpc`: magic `FSLPROF1`, version u16=1, flags u16=0, 32 source-hash bytes,
profile count u16; all integers little endian. Each record contains five
u16-length strings (ID, processor, variant, decoder, package hash), format u8=1
(ELF), machine u16, bitness u8, endian u8=1/2, flags mask u32 and value u32.

Limits: 1 MiB source/package, 1..1024 profiles, identifiers 1..128 bytes,
32/64-bit ELF targets, lowercase 64-character package hashes. Malformed/trailing
data, unknown versions/tags, invalid masks, duplicate IDs and overlaps refuse.
The source hash is provenance, not authentication.

## Entry points

`LoaderPipeline::load_with_catalog` and `LoadedBinary::from_file_with_catalog`
provide explicit owned policy. Existing `load`, `from_file` and
`load_format_facts` remain available. New format coverage is ELF only; other
formats refuse without a legacy fallback.

```sh
cargo build --locked -p fission-fsl --features x86 --bin fsl-x86
fsl-x86 compile-profiles elf.fslprofiles elf.fslpc
fsl-x86 inspect-profiles elf.fslpc
fsl-x86 select-profile elf.fslpc INPUT.elf
fsl-x86 decompile-profiled elf.fslpc INPUT.elf ADDRESS SIZE c NEW_DIRECTORY
```

The last command reuses the existing span mapper, x86 package, FIR and backend.
The report records profile/catalog/package identities, target facts and
unselected ABI. It still requires a caller-supplied executable span and produces
explicit-state code, not recovered source functions. This mechanical loader
change does not establish decode, execution parity or decompiler quality gains.

Next: import complete profile/selection dependencies; model compiler/ABI
ambiguity; resolve register/layout/fixup contracts; extend format routes through
canonical parsers; propagate profiles to consumers; validate and migrate each
consumer. Final SLEIGH removal follows the complete migration gates.
