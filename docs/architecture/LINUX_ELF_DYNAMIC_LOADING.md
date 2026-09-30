# Linux ELF dynamic loading in the emulator

The Linux emulator checks ELF64 little-endian `e_machine` before it chooses a
dynamic-link path. Dynamic x86-64 images use the existing HLE GOT, mini-library,
or opt-in interpreter paths. Dynamic AArch64 images use the HLE GOT path.
Unsupported machine values fail with the numeric `e_machine` in the diagnostic;
the x86-64 relocation handler also rejects non-x86-64 images directly.

## AArch64 HLE scope

The initial AArch64 path reads relocation tables through `PT_DYNAMIC` and
supports `R_AARCH64_RELATIVE`, `R_AARCH64_ABS64`, `R_AARCH64_GLOB_DAT`, and
`R_AARCH64_JUMP_SLOT`. It applies locally defined symbols and imports resolved
from the main image. Unresolved `GLOB_DAT` and `JUMP_SLOT` entries are left for
`LinuxEnv` to patch to registered HLE procedures before execution. Relocation
records are fully validated before any writes are committed; an unsupported
type or malformed table leaves the mapped image's relocation slots unchanged.

This mode is the documented AArch64 interpreter/library route today: it does
not open host libraries by soname and does not load a guest `PT_INTERP` or
`DT_NEEDED` library. Imported functions must have a Linux HLE procedure. Setting
`FISSION_ENABLE_DYNLINK=1` for an AArch64 image with `PT_INTERP` returns an
explicit unsupported-mode diagnostic. A guest filesystem root and AArch64
interpreter execution are future work.

The checked-in `crates/fission-emulator/testdata/aarch64_dyn_import.elf` fixture
contains one relative data pointer and one imported `puts` call. The test runs
that fixture only in Fission and checks that the HLE import and exit syscall
were handled.

## x86-64 scope

x86-64 retains the existing HLE GOT and mini-dynlink paths. Host libraries are
searched only on this architecture, and mapped interpreters and libraries are
validated as x86-64 before their segments are installed. Interpreter segment
mapping is staged so a bad header or segment cannot leave a partial mapping.
