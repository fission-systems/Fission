# Emulator testdata sources

## Dyn HLE fixtures (checked in, CI)

```bash
zig cc -target x86_64-linux-musl -Os -dynamic -s \
  -o testdata/x64_dyn_printf_malloc.elf testdata/src/printf_malloc.c
```

The AArch64 dynamic fixture is a safe shared-object image that calls the HLE
`puts` procedure and exits. It is only executed by Fission's emulator test; it
is not intended to run on the host.

```bash
zig cc -target aarch64-linux-musl -nostdlib -fPIC -shared \
  -Wl,-e,_start -Wl,--build-id=none \
  -o testdata/aarch64_dyn_import.elf testdata/src/aarch64_dyn_import.S
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

# Linux guest task fixture

`linux_guest_futex.S` is a libc-free x86-64 Linux ELF that clones one guest
thread and coordinates with `FUTEX_WAIT_PRIVATE` / `FUTEX_WAKE_PRIVATE`. Build
it from this directory with:

```sh
zig cc -target x86_64-linux-musl -static -nostdlib -fno-pie -no-pie \
  -Wl,-e,_start -Wl,--build-id=none -o ../linux_guest_futex.elf \
  linux_guest_futex.S
```

The test runs the checked-in ELF only inside Fission; it does not execute the
fixture on the host.

## AArch64 generic `readlinkat` fixture

`linux_aarch64_readlinkat.S` checks the `/proc/self/exe` guest symlink and
stores the returned byte count and path in exported data for the smoke test.
Build the checked-in static guest with:

```sh
zig cc -target aarch64-linux-musl -nostdlib -static -fno-pie -no-pie \
  -Wl,-e,_start -Wl,--build-id=none -o ../linux_aarch64_readlinkat.elf \
  linux_aarch64_readlinkat.S
```

Execute it only in Fission's emulator.
