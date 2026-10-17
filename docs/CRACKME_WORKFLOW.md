# Crackme workflow: evidence to an executed verdict

Fission can support a manual solve of the public Reversing-ELF `crackme4`,
including a copy with its symbols stripped. This is a measured beginner
workflow, not an automatic solver or a claim of general crackme coverage.
Use the same sequence for another authorized challenge and report blockers.

## 1. Pin the input and inspect it

Input provenance: [Reversing-ELF binary](https://github.com/anurag708989/Reversing-ELF/blob/cf23afa0248fc43aac0b1c31c58b59aafe7e7326/crackme4),
commit `cf23afa0248fc43aac0b1c31c58b59aafe7e7326`, SHA-256
`a071c14459c27f71a226476e35e2e5b4e435c4198df995ec5f4b8e99bd671a92`.
The external binary is kept outside Git; the regression fixture is Fission-owned.

```sh
curl -L --fail -o /tmp/crackme4 \
  https://raw.githubusercontent.com/anurag708989/Reversing-ELF/cf23afa0248fc43aac0b1c31c58b59aafe7e7326/crackme4
shasum -a 256 /tmp/crackme4
fission_cli info /tmp/crackme4 --json
fission_cli strings /tmp/crackme4 --contains 'password' --context --json
```

The success string at `0x4007e8` is referenced by the instruction at `0x4006de`
in the discovered function at `0x40067a`. Its call relationships include
`strcmp` and a local function at `0x40062d`. Follow the reported addresses;
these values describe this sample, not conditions in the implementation.

## 2. Follow the check and decode the input

```sh
fission_cli callgraph /tmp/crackme4 --function 0x40067a --json
fission_cli disasm /tmp/crackme4 --addr 0x40067a --function --json
fission_cli disasm /tmp/crackme4 --addr 0x40062d --function --json
```

The first function constructs 18 bytes on its stack from little-endian
immediates, transforms that buffer, then compares it with argv[1]. The local
callee walks bytes until the NUL and XORs each with `0x24`. The assembled bytes
and the observed byte operation yield the candidate:

```python
encoded = bytes.fromhex('495d7b491456177b5741475156177b545340')
print(bytes(byte ^ 0x24 for byte in encoded).decode())
# my_m0r3_secur3_pwd
```

The candidate was derived from Fission's disassembly, without source or a
published solution. Removing the symbol table with `llvm-strip --strip-all`
preserves this navigation and execution result; function names become empty.

## 3. Verify the actual verdict

```sh
fission_cli sandbox /tmp/crackme4 --json --max-inst 10000 \
  --fail-on-budget --max-hle-misses 0 --max-unknown-syscalls 0 \
  -- my_m0r3_secur3_pwd
```

Require a guest exit, inspect `guest_output.stdout` as bytes and check the
success message. This crackme returns status **0 on both paths**, so exit status
alone is insufficient. Check wrong, truncated, extended and empty inputs too.
`sandbox --json` emits one JSON document; guest bytes are captured in that
report, while host diagnostics remain on stderr. Text mode retains console echo.

A reusable gate records the static evidence, each sandbox command and an
optional native Linux oracle:

```sh
python3 scripts/test/run_crackme_workflow.py \
  --cli target/release/fission_cli --binary /tmp/crackme4 \
  --candidate my_m0r3_secur3_pwd --success-text 'password OK' \
  --reject wrong-password --reject my_m0 --reject my_m0r3_secur3_pwdX --reject '' \
  --oracle-image python:3.12-slim-bookworm@sha256:34386ef0cb081344d7ec1c103ba398e6e9f64e9ab3a1509accc92a4e24a07258 \
  --artifacts benchmark/artifacts/crackme/original
```

The oracle runs the supplied binary in an ephemeral Linux/amd64 Docker
container. Docker is a validation dependency only, not a Fission runtime
backend. The workflow does not discover a password automatically: supply a
candidate derived from analysis. It rejects budget overruns, incomplete
console capture, missing exits and differing native bytes/status. It records
input/CLI hashes and verifies the input was unchanged.

## Supported execution and limits

Default x86-64 Linux HLE startup installs a bounded thread-header guard from
that guest's AT_RANDOM bytes before transferring to main. It synchronizes the
existing FS setter and preserves an already installed thread pointer. Static
CRT, shared-library and interpreter initialization stay with their existing
owners. ABI layout constants belong to Linux ABI data; no function or sample
identity controls this behavior.

This implements guard/header storage, **not** PT_TLS module loading, a DTV,
full pthread state or a complete libc loader. Guard failure terminates with
conventional status 134; guest SIGABRT delivery, signal handlers and core dumps
are not modeled by that HLE. Other ABIs do not receive this x86-64 layout.

The console report exposes retained VFS descriptor 1/2 contents as byte arrays.
An unavailable descriptor is `null` with a stream error, never evidence of no
output. It is not a transcript of arbitrary descriptor redirection/replacement.
Opaque indirect checks, anti-debugging, packing, native debugger parity and
automatic symbolic password solving remain separate work.
