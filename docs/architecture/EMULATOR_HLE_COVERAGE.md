# Emulator guest HLE coverage

The emulator records the number a Linux guest issued separately from the
canonical syscall number used by its shared handler registry. Coverage output
names each guest syscall, includes its guest ABI and binary-relative path, and
records how many calls had no implementation. Windows API misses keep the
imported API name. The process exit status is a separate field: a clean exit
does not erase an unsupported operation observed earlier.

## Safe corpus report

`crates/fission-emulator/tests/corpus_dev_sweep.rs` is the full measurement
lane. It only executes Fission's source-built dev binaries. Do not point it at
DecBench, evalkit, malware, or another corpus that is intended for static
analysis only.

```sh
FISSION_SWEEP_ROOT=/Users/sjkim1127/fission-benchmark/corpus/dev/binaries \
FISSION_HLE_REPORT_PATH=/tmp/fission-hle-coverage.json \
cargo test --release -p fission-emulator --test corpus_dev_sweep \
  -- --ignored --nocapture
```

`FISSION_HLE_REPORT_PATH` writes deterministic JSON with schema version 1.
Each binary row contains `guest_os`, `guest_abi`, `process_status`,
`exit_reason`, `exit_code`, instruction count, guest syscall number/name/count,
per-syscall `unhandled_count`, and named API misses/counts. Binary paths are
relative to the selected corpus root. Rows and counters are ordered so the
same input and emulator revision produce byte-stable JSON.

## Bounded CI smoke

`crates/fission-emulator/testdata/hle_smoke_manifest.json` defines the small
CI lane and per-binary limits for instructions, unknown syscalls, and API
misses. The AArch64 fixture calls generic-table `readlinkat(78)` for
`/proc/self/exe`, checks its return value, and stores the result buffer for the
test to compare with the guest executable path. An absolute guest `execfn` is
preserved unless it is the loaded host binary path; host paths and relative
`argv[0]` values get a stable `/fission-guest/<basename>` path. The emulator
never reads a host symlink or exposes the loaded host binary path through this
virtual link.

## Explicit unsupported scope

The measured AArch64 dev-corpus misses `set_robust_list` (generic syscall 99)
and `rseq` (293) remain unsupported and return `ENOSYS`. Their names and
per-binary counts remain in the report. `getdents64` (generic syscall 61) and
`pipe2` (generic syscall 59) also remain unsupported. Generic calls without a
known name use the `syscall_<n>` fallback.

## Measured snapshot (2026-09-30)

The safe dev corpus contained 148 candidate binaries at benchmark checkout
`70e4c6348db412bcde31d17ae5f1854829d1db09`. All 148 reached `process_exit`.
The legacy API-only count remains 130 binaries without a named API miss; when
unhandled syscalls are included, 128 had no syscall or API misses. The only
unhandled Linux calls were AArch64 `set_robust_list` and `rseq`, twice each.
No P-Code opcode or CALLOTHER remained unhandled.

The remaining Windows API misses were `RegCloseKey`, `RegOpenKeyExA`,
`_difftime64`, `_localtime64`, `div`, and `strtol` (7 each);
`AddVectoredContinueHandler` and `AddVectoredExceptionHandler` (4 each); and
`CreateWaitableTimerExW`, `GetErrorMode`, `SetErrorMode`,
`SetThreadDescription`, `SetThreadStackGuarantee`, `WerGetFlags`,
`WerSetFlags`, and `timeBeginPeriod` (2 each). These counts are call totals
across the 148 runs.

The sorted candidate paths and their file SHA-256 digests produce inventory
SHA-256 `ad66b140a457b7e2e788e45fc5be931fdf78891d3a5ff87f3c17584a6ad53acc`
when each record is encoded as `relative-path NUL file-digest LF`.

## Measured snapshot (2026-11-20)

The dev corpus binaries are not in any repository -- only their sources and
manifests are -- and the benchmark checkout no longer carries the builder.
They were rebuilt from benchmark commit
`70e4c6348db412bcde31d17ae5f1854829d1db09` with its own `corpus-builder` image:

```sh
git -C ~/fission-benchmark archive 70e4c6348db412bcde31d17ae5f1854829d1db09 | tar -x -C <dir>
cd <dir>
CORPUS_TARGET=windows-x86_64 docker compose --profile tools run --rm \
  corpus-builder python3 scripts/build_matrix.py --split dev --languages c,cpp,rust,go
```

Run only the `windows-x86_64` target: a `host` pass rebuilds the gcc C
variants as ELF under their `.exe` names before it stops on the missing
`gcc-m32` tool, and the sweep picks the guest OS by extension.

That produced 96 binaries, not 148, so the two snapshots are not the same
denominator. On `main` (`6b36af408`) 91 of 96 exited, but exiting is not
running correctly. Each C program's exit code was compared with the same
source compiled and run natively: 68 of the 76 C builds agreed. The JIT
reused stale four-byte views of a sixteen-byte temporary (the second
`movaps` in a block stored the first one's value), which gave
`crypto` (clang `-O2`) and `memory_layouts` (gcc `-m32`, twice) a wrong
result. The two Go binaries were counted as exits but were the Go runtime
aborting before `main`.

After the fixes in this change: 96 of 96 exit, all 76 C builds agree with
the native run, and 76 binaries have no syscall or API miss. The Go
binaries get past five start-up stops (system directory, `KUSER_SHARED_DATA`,
the environment block, `DuplicateHandle`, a zero `nanotime`) and now stop
at `CreateEventA`; past that the runtime waits on threads it started, which
the Windows layer does not run. The only unhandled Linux calls are still
AArch64 `set_robust_list` and `rseq`.
