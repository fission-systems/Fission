# CSIC fixtures

Small C programs written for the CSIC P1 evaluation (RFC #1101, Track B).
Each reads its request from standard input and makes its sink visible
natively on stdout and in the exit status, so a native run can confirm
what the emulator replays (`../native/verify_native.py`).

| Source | Sink | Ground truth (input bytes) |
|---|---|---|
| `rfc_auth.c` | `session.is_admin == 1` at exit | `req_buf[3]`; bytes 8..11 are logged through a variable index (static false alias) |
| `implicit_call.c` | `grant(7)` | `req[5]` (implicit flow into a constant argument) |
| `false_alias_ptr.c` | `cfg.flag == 1` at exit | `req[0]`; bytes 4..7 are stored through a pointer parameter |
| `real_alias.c` | `account.is_admin != 0` at exit | `req[2]` (store offset) and `req[9]` (stored value) |
| `joint_or.c` | `grant()` called | none alone; `{req[0], req[1]}` jointly |
| `joint_and.c` | `grant()` called | `req[0]` and `req[1]`, each alone |
| `nondet_time.c` | `grant()` called | depends on the clock: must be refused |
| `nondet_random.c` | `grant()` called | depends on `getrandom`: must be refused |
| `crash_null.c` | fault at the store | `req[0]` (selects a NULL slot); not `req[1]` (the stored value) |
| `password.c` | `grant_admin(1000)` | `req[0..3]` |
| `branch_len.c` | `classify`'s first branch outcome | `req[0]` |
| `and3.c` | `grant()` called | on "abcQ": joint `{req[0..3]}` = "KEY"; `req[3]` is a decoy on the path |
| `xor_gate.c` | `grant()` called | `req[0]` and `req[1]` each alone; flipping the same bit in both restores Φ |
| `segv_grant.c` | `grant()` in the SIGSEGV handler | `req[0]` (selects the NULL slot); not `req[1]` (the stored value) |
| `carry.c` (`-DCARRY=1`) | `grant()` called | joint `{req[0], req[2]}` or `{req[1], req[2]}` (a carry out of a stored sum, and 'W') |
| `carry.c` (`-DCARRY=2`) | `grant()` called | none: the high byte of a two-byte sum never reaches 2 |

Rebuild with `./build.sh` (zig 0.15.2, `llvm-objcopy`). Builds are static
musl, `-fno-pie`, symbols kept, debug info stripped: x86-64 `-O0` for every
source, x86-64 `-O2` for `rfc_auth.c`, AArch64 `-O0` for `rfc_auth.c`,
`crash_null.c` and `password.c`. P2 adds x86-64 `-O0` for `and3.c`,
`xor_gate.c`, `segv_grant.c` and both `carry.c` variants, x86-64 `-O2` for
`xor_gate.c`, and AArch64 `-O0` for `and3.c` and `segv_grant.c`.
