# Throughput, measured against the other two engines

Three emulators, the same two byte sequences, the same instruction counts, and
the same two-point method so translation and set-up fall out of the difference.

| workload      | Unicorn (QEMU TCG) |  Fission | Ghidra `PcodeEmulator` |
|---------------|-------------------:|---------:|-----------------------:|
| register loop |          264.0 M/s | 19.5 M/s |               0.23 M/s |
| memory loop   |          120.4 M/s | 10.0 M/s |               0.22 M/s |

Measured 2026-09-10 on an Apple Silicon host. Unicorn 2.1.4 built from
`vendor/`, Ghidra 11.4.2 headless.

Fission's column moved during the session that produced this file: 8.28 / 4.74
before the p-code temporaries stopped being flushed to memory at every block
exit, 19.5 / 10.0 after. The gap to Unicorn went from 32x to 13.5x on the
register loop and 25x to 12x on the memory one.

## Reproducing

```bash
# Fission
cargo test --release -p fission-emulator --test throughput_bench -- --ignored --nocapture

# Unicorn -- QEMU's TCG as a library, which is the fair comparison: same job,
# no device emulation. qemu-user itself is Linux-only, so it cannot be the
# thing measured on a macOS host.
cmake -S vendor/unicorn-2.1.4 -B /tmp/uc-build -DCMAKE_BUILD_TYPE=Release \
      -DUNICORN_ARCH=x86 -DBUILD_SHARED_LIBS=OFF
cmake --build /tmp/uc-build -j
cc -O2 -Ivendor/unicorn-2.1.4/include bench/unicorn_throughput.c \
   /tmp/uc-build/libunicorn.a -o /tmp/uc_bench -lpthread -lm && /tmp/uc_bench

# Ghidra
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
mkdir -p /tmp/gscripts && cp bench/ghidra_ThroughputBench.java /tmp/gscripts/ThroughputBench.java
vendor/ghidra/ghidra_11.4.2_PUBLIC/support/analyzeHeadless /tmp/gproj bench \
  -scriptPath /tmp/gscripts -preScript ThroughputBench.java -noanalysis
```

## What the numbers say

The register loop touches no memory at all and is still the *worse* of the two
ratios, so the gap is not mainly the memory path -- it is per-instruction work.

Two guesses at what that work was turned out to be wrong, and the profile named
the real one. Dead flag elimination bought 2%; letting blocks chain under an
instruction budget bought nothing. What the profile actually showed, in a loop
with no memory access at all, was `im::hamt::hash_key`, `SipHasher::write`,
`malloc`/`free` and `Arc::make_mut` at the top -- p-code *temporaries* being
written out to `MachineState` at every block exit. SLEIGH scopes a temporary to
one instruction and a block never splits an instruction, so nothing outside the
block could ever read them. Not writing them is 2.1-2.35x.

The eager flags are still there and still real -- SLEIGH lifts every x86
arithmetic instruction with all six, parity included, so `add eax, 1` is about
ten p-code ops of which seven are flags and one is a `PopCount`, and QEMU
computes none of them until something reads (`cc_op`/`cc_src`/`cc_dst`). The
dead-value pass now removes the ones that are overwritten before any read. It
was simply not where the time was going. SLEIGH's x86 lifts every arithmetic
instruction with all six flags, parity included, so `add eax, 1` is around ten
p-code ops of which seven are flags and one is a `PopCount`. QEMU's TCG keeps
`cc_op`/`cc_src`/`cc_dst` and computes a flag only when something reads it; in
`add eax,1; sub ecx,1; jnz`, every flag the `add` computes is overwritten
before anything reads it.

Ghidra's emulator is a p-code interpreter on the JVM and is not built for
throughput, so this comparison is not what it is for. It is here because it
bounds the other side: interpreting the same p-code costs about 1/30th of
compiling it.
