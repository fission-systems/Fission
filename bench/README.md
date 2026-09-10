# Throughput, measured against the other two engines

Three emulators, the same two byte sequences, the same instruction counts, and
the same two-point method so translation and set-up fall out of the difference.

| workload      | Unicorn (QEMU TCG) | Fission  | Ghidra `PcodeEmulator` |
|---------------|-------------------:|---------:|-----------------------:|
| register loop |          264.0 M/s | 8.28 M/s |               0.23 M/s |
| memory loop   |          120.4 M/s | 4.74 M/s |               0.22 M/s |

Measured 2026-09-10 on an Apple Silicon host. Fission at `318172eb9`, Unicorn
2.1.4 built from `vendor/`, Ghidra 11.4.2 headless.

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

The register loop touches no memory at all and is the *worse* of the two ratios
(32x behind Unicorn against the memory loop's 25x). So the gap is not mainly
the memory path -- it is per-instruction work, and the largest single piece of
that is **eager flag computation**. SLEIGH's x86 lifts every arithmetic
instruction with all six flags, parity included, so `add eax, 1` is around ten
p-code ops of which seven are flags and one is a `PopCount`. QEMU's TCG keeps
`cc_op`/`cc_src`/`cc_dst` and computes a flag only when something reads it; in
`add eax,1; sub ecx,1; jnz`, every flag the `add` computes is overwritten
before anything reads it.

Ghidra's emulator is a p-code interpreter on the JVM and is not built for
throughput, so this comparison is not what it is for. It is here because it
bounds the other side: interpreting the same p-code costs about 1/30th of
compiling it.
