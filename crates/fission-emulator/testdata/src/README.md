# Emulator testdata sources

## Dyn HLE fixtures (checked in, CI)

```bash
zig cc -target x86_64-linux-musl -Os -dynamic -s \
  -o testdata/x64_dyn_printf_malloc.elf testdata/src/printf_malloc.c
```

## Static musl (opt-in CI via FISSION_SMOKE_STATIC_PRINTF=1)

```bash
zig cc -target x86_64-linux-musl -O1 -static -s \
  -o testdata/x64_static_printf_malloc.elf testdata/src/printf_malloc.c
```

## Tiny syscall-only concolic fixture

`x64_concolic_branch_sys.elf` is a freestanding hand-built ELF (read/cmp/exit).
Regenerate with the Python snippet in the smoke test history or:
`testdata/src/build_concolic_sys.py` (if present).
