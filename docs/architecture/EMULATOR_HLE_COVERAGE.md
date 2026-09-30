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
