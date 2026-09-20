# Mach-O Thread-Command Entry Points

## Baseline / issue anchor

- Issue: #42
- Owner: `crates/fission-loader/src/loader/macho/`
- Current defect: the loader records an entry point only from `LC_MAIN`, so
  Mach-O images that use `LC_UNIXTHREAD`/`LC_THREAD` retain entry point `0`.
- Observable invariant: when no `LC_MAIN` entry is present, the first matching
  architecture thread-state program counter is the Mach-O entry point.

## Owner proof

Both `MachoLoader::parse_64` and `MachoLoader::parse_32` own the load-command
walk and initialize `entry_point`. `schema.rs` defines `LC_MAIN` but no thread
commands or thread-state decoding, so the missing entry point is created at the
loader boundary before downstream discovery can run.

## Generalized rule

Recognize `LC_UNIXTHREAD` and `LC_THREAD` in the same load-command walk. Decode
the target architecture's documented thread-state flavor and program-counter
slot for x86, x86-64, ARM, and ARM64. Use the result only as a fallback so an
explicit `LC_MAIN` value remains authoritative. Unknown architectures and
unsupported thread-state flavors return no fallback rather than guessing.

No binary, address, file-name, or OS-version guard is needed.

## Validation matrix

- Synthetic thread commands recover the PC for x86, x86-64, ARM, and ARM64.
- Unknown architectures and malformed/truncated state do not produce a
  fabricated entry point.
- Existing Mach-O loader tests remain green.
- `cargo nextest run -p fission-loader`, downstream decompiler/CLI checks,
  format, and diff checks.
