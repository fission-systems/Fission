# Loader format coverage

What `fission-loader` detects and how deep each format goes, and the order in
which new formats are worth adding. The route table is
`crates/fission-loader/src/loader/pipeline.rs` (`LoaderPipeline::route`); each
row below links the depth to the loader that owns it. Depth is read from the
code; "verified" means a test or a real file exercises it.

## Inventory

Baseline taken 2026-11-26; rows changed since say so.

Depth columns: **Sec** sections/segments mapped, **Sym** symbols, **Rel**
relocations into `RelocationsDb`, **Imp** imports/exports, **Ent** entry point,
**Dbg** debug info.

| Format | Detection | Loads? | Sec | Sym | Rel | Imp | Ent | Dbg | Notes |
|---|---|---|---|---|---|---|---|---|---|
| PE32 / PE32+ (`pe/`) | `MZ` + `PE\0\0` at `e_lfanew` | yes | yes | yes (COFF table, exports) | base relocs | IAT, delay-load, exports | yes | PDB sidecar, CodeView | Rich header, SEH/LSDA, MinGW pseudo-relocs, CRT entry |
| ELF 32/64, LE/BE (`elf/`) | `\x7fELF` | yes | sections, or `PT_LOAD` when there are none | `.symtab`/`.dynsym`, versions | REL/RELA, ET_REL patching | PLT/GOT imports | yes | DWARF, `.gnu_debuglink`/build-id companions, eh_frame/LSDA | ET_REL (`.o`, `.ko`) maps sections at synthetic bases |
| Mach-O 32/64 (`macho/`) | thin magics | yes | yes | yes | yes | indirect symbols (stubs, lazy/non-lazy pointers) | `LC_MAIN`/`LC_UNIXTHREAD` | DWARF | ObjC/Swift analyzers; no filetype gate, so kexts take the generic path (not verified on a real kext) |
| Universal (fat) Mach-O | `FAT_MAGIC`/`FAT_CIGAM` | yes, one slice | per slice | per slice | | | | | CLI `--arch` picks the slice; a slice that is not Mach-O (fat `.a`) fails like a thin one |
| COFF object (`coff/`) | i386/AMD64/ARM/ARM64 machine + sane header | yes | yes | yes | **no** (`IMAGE_RELOCATION` not read) | externals | none | no | bigobj (`0000 FFFF` header) and import objects are not detected |
| UEFI TE (`formats/te.rs`) | `VZ` | yes | yes | header types | base relocs | | yes | | |
| Raw image (`formats/raw.rs`) | never detected | with CLI `--base` + `--language` | one section | entry only | | | `--entry` | | |
| Intel HEX (`formats/hex.rs`) | `:` + hex digits | with an explicit language (CLI `--language`); without one, `LoadSpecNotFound` | one per contiguous run | entry only | | | start record (type 03/05) when it lands on a record | | baseline: never loaded, although the 1.0 matrix listed it as best effort. Loaded data is the decoded image (`formats/sparse.rs`) |
| Motorola S-record (`formats/hex.rs`) | `S0`-`S9` line | as Intel HEX | one per contiguous run | entry only | | | S7/S8/S9 when it lands on a record (Thumb bit kept) | | as Intel HEX |
| MZ (DOS) (`formats/mz_ne.rs`) | `MZ` without a PE/NE header | **no**: header parsed, then `LoadSpecNotFound` | | | | | | | |
| NE (Win16/OS2) | `MZ` + `NE` | **no**: segments parsed, then `LoadSpecNotFound` | | | | | | | |
| Unix a.out (`formats/aout.rs`) | OMAGIC/NMAGIC/ZMAGIC, LE or BE | **no**: header parsed, then `LoadSpecNotFound` | | | | | | | |
| ELF core dump | `\x7fELF`, `ET_CORE` | yes, as plain ELF | `PT_LOAD` | | | | 0 | | notes (`NT_PRSTATUS`, `NT_FILE`) ignored; by code reading, no test |
| Unix `ar` (`.a`, MSVC `.lib`) | `!<arch>\n` | no: `ContainerRequiresExtraction(UnixArchive)` | | | | | | | members are never enumerated |
| ZIP, gzip, xz, 7z, RAR, CAB, OLE CFB | magics (`containers/`) | no: `ContainerRequiresExtraction(..)` | | | | | | | |
| Ghidra families (GZF, dyld cache, PEF, SOM, OMF) | magics | no: `UnsupportedLoaderFamily(..)` | | | | | | | |
| UF2 (`formats/uf2.rs`) | three block magics in the first 512 bytes | with an explicit language; without one, `LoadSpecNotFound` naming the family IDs | one per contiguous run | | | | none (UF2 has no start address) | | added after the baseline (was "unknown"). `NOT_MAIN_FLASH` blocks skipped; file containers refused; families that disagree about an address refused |
| anything else (WebAssembly, `.class`, DEX, XBE, LE/LX, ...) | none | no: `UnsupportedFormat: unknown binary format` | | | | | | | |

An unsupported input always fails closed with a typed message
(`UnsupportedFormat`, `ContainerRequiresExtraction(..)`,
`UnsupportedLoaderFamily(..)`, `LoadSpecNotFound`, `MalformedHeader`); nothing
falls back to a raw load. `fuzz_any_format` drives every route from raw bytes.

SLEIGH specs shipped in `utils/sleigh-specs/compiled/`: 6502, 68000, 8048,
8051, 8085, AARCH64, ARM, Atmel (AVR), BPF, CP1600, CR16, Dalvik, HCS08, HCS12,
Hexagon, JVM, Loongarch, M16C, M8C, MC6800, MCS96, MIPS, NDS32, PA-RISC, PIC,
PowerPC, RISCV, Sparc, SuperH, SuperH4, TI_MSP430, V850, Xtensa, Z80, eBPF,
tricore, x86.

## Proposed order

Ranked by value to a reverse engineer times fit with the specs above, cheapest
first within a rank.

1. **Intel HEX and Motorola S-record, loadable with an explicit language.** The
   parsers exist and are fuzzed; only the last step refuses. These are the
   usual delivery format for microcontroller firmware (8051, AVR, PIC, MSP430,
   Cortex-M, 68k, Z80), and every one of those has a shipped spec. The file
   carries no architecture, so the language stays the caller's (`--language`),
   exactly as for a raw image. Fail closed without it. **Done** (#1404).
2. **UF2.** The flashing format of RP2040/RP2350, nRF52, SAMD, ESP32-S2/S3 and
   STM32 boards. 512-byte blocks with a target address each: the same sparse
   image as HEX, so it reuses item 1's mapping and language path. The family
   ID names the chip, not the instruction set variant, so it is reported, not
   used to pick a language. **Done** (second PR).
3. **Unix `ar` archives (`.a`, MSVC `.lib`).** Static libraries are the most
   common input Fission refuses outright. Enumerate members (GNU and BSD long
   names, the MSVC/GNU symbol-index members) and load a chosen member through
   the existing ELF/COFF object loaders; short import objects in `.lib`s are
   identified, not loaded.
4. **COFF object relocations.** `IMAGE_RELOCATION` records are ignored, so a
   `.obj`'s calls to externals have no targets. Same depth as ELF ET_REL.
5. **ELF core dumps.** Already map; read `NT_PRSTATUS` (registers at the
   fault) and `NT_FILE` (which file each mapping came from) into typed facts.
6. **MZ (16-bit real mode x86) and a.out.** Parsed already; need a load spec
   per header (`x86:LE:16:Real Mode`; a.out machine id to x86/68k/SPARC) and a
   check that 16-bit segmented lifting produces usable output before it is
   offered.
7. **Identify-only, typed refusal.** WebAssembly (no SLEIGH spec, so no lifting
   path), JVM `.class` and Android DEX (JVM and Dalvik specs ship, but their
   constant-pool operations need a loader-provided pool the SLEIGH runtime does
   not have), .NET assemblies (CIL has no spec; the native PE stub already
   loads). Each gets a named `UnsupportedLoaderFamily` so a caller learns what
   the file is instead of "unknown binary format". Lifting is a separate
   project for each and is not implied.

Dropped: Xbox XBE, LE/LX and NE (narrow audience; NE and LE also need 16-bit
segmented addressing), Mach-O kexts as a separate item (they already take the
Mach-O path).
