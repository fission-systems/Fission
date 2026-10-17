# Real crackme workflow validation — 2026-10-12

Baseline: main `9ef91080980b1f86e89ea51e404176dfd3c04b2a`.
A clean isolated worktree and checkout-local Cargo target were used. Primary
checkout changes, the prior decompiler edits and PR #176 are preserved.

## Observed blockers and owners

Public input: Reversing-ELF `crackme4`, pinned commit
`cf23afa0248fc43aac0b1c31c58b59aafe7e7326`, SHA-256
`a071c14459c27f71a226476e35e2e5b4e435c4198df995ec5f4b8e99bd671a92`.
The candidate was recovered from Fission strings/context, callgraph and disasm
only, without reading source or a published solution. The encoded stack bytes
are transformed by a byte XOR loop before comparison. See
[the walkthrough](../CRACKME_WORKFLOW.md) for the evidence and reproducible steps.

- #198: baseline sandbox reaches a guest FS:0x28 read and fails with an unmapped
  RAM page. Linux HLE bypasses libc setup without supplying its guard state.
  The fix belongs to Linux libc startup with layout facts in Linux ABI data.
  Actual AT_RANDOM bytes provide the guards; an installed pointer and non-HLE
  initialization are preserved. Stack-check failure must never return success.
- #199: guest printf/puts output appears before the baseline JSON document.
  Its contents are also absent from the report. The fix uses the existing host
  echo control and VFS capture, attaching bytes to the typed sandbox report.
  No separate output collector or semantic repair at the CLI is introduced.

The glibc x86-64 thread-header ABI and guard seeding contract were consulted as
primary references; vendor QEMU FS-state setup was consulted for ownership.
References: [glibc thread header](https://github.com/bminor/glibc/blob/master/sysdeps/x86_64/nptl/tls.h),
[glibc guard seed contract](https://github.com/bminor/glibc/blob/master/sysdeps/unix/sysv/linux/dl-osinfo.h).
No vendor code or runtime/build dependency was introduced.

## Measured execution

| Input form | Candidate | Wrong | Truncated | Extended | Empty |
| --- | --- | --- | --- | --- | --- |
| Original x86-64 ELF | accepted | rejected | rejected | rejected | rejected |
| Symbol-stripped copy | accepted | rejected | rejected | rejected | rejected |

All ten sandbox runs matched the Linux process oracle's exact stdout, stderr
and exit status. Both paths return zero in this challenge; the output message
is the verdict. Accepted inputs executed 417 instructions and rejected inputs
419; all halted before the 10,000 instruction bound. Unimplemented opcodes,
HLE misses, unknown syscalls and unhandled userops were zero. Input hashes were
unchanged. Static navigation located the validator in both input forms.

The stripped SHA-256 is
`e8b549ad4bc6d674a0dba0f28c7cfdda21f9d02aac439d3080bc5e11728f3060`.
Docker oracle image was pinned to the manifest digest
`sha256:34386ef0cb081344d7ec1c103ba398e6e9f64e9ab3a1509accc92a4e24a07258`.
The host was macOS arm64; Docker ran Linux/amd64 guest processes. This is actual
Linux process execution, not native macOS debugging or physical x86 CPU parity.
Docker is used only for validation, not as a product execution dependency.

## Local validation

- Focused nextest: **10 passed** (guard initialization/preservation/errors,
  JIT normal/corrupt guard paths, byte capture, Linux libc/syscall and Windows
  console JSON, text compatibility and metrics-file parity).
- Full emulator + CLI nextest: **379 passed, 3 pre-existing ignored tests**.
  Ignored: dev corpus sweep, throughput bench and benchmark translation sweep;
  these runs do not assert corpus-wide emulator completeness.
- Affected-crate check and clippy with CI policy: passed.
- Locked release CLI build, workspace fmt and diff checks: passed.
- Generic workflow replay: ten cases above, with static evidence retained.
- Existing project/CLI smoke: 43 commands + two unchanged input hashes passed.
  Reusing its old output directory initially failed the expected revision-1
  assertion because the prior database remained. A fresh directory passed;
  no production change was made in response to that harness-state failure.

The owned ELF assembly fixture executes only inside Fission and ensures a
corrupt saved guard cannot return past __stack_chk_fail. Status 134 is a bounded
HLE termination contract; guest signal handlers/core dumps are not simulated.
Guard source tests modify the guest's random bytes to exclude constant seeding.

Commands:

```sh
cargo nextest run -p fission-emulator -p fission-cli \
  -E 'test(hle_runtime_guard) | test(stack_check_failure) | binary(hle_runtime_guard) | binary(sandbox_output_cli)'
cargo nextest run -p fission-emulator -p fission-cli --status-level all
cargo check -p fission-emulator -p fission-cli
cargo clippy -p fission-emulator -p fission-cli --locked -- \
  -D warnings -A clippy::style -A clippy::complexity
cargo build -p fission-cli --release --locked
cargo fmt --all --check
```

Logs, captured commands, binary/source hashes and oracle results are in the
primary checkout's ignored `benchmark/artifacts/re-workspace/2026-10-12-crackme/`.
The workflow, source fixture, regression tests and this report are committed.

## Limits

One public beginner crackme plus its derived stripped copy was solved; these
are not independent challenges. There is no automatic password recovery,
packing/anti-debug support claim, full DTV/PT_TLS/pthread implementation,
ELF32 execution proof or decompiler quality/ranking claim. The downloaded
ELF32 crackme3 was not used for a solve or execution claim. Other architectures
remain on their existing startup contracts. Guest console capture reflects
retained VFS descriptors and reports unavailable capture explicitly.
