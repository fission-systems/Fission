# Static x86-64 fixture for measuring FID

DecBench's binaries are dynamically linked, so FID has nothing to match there.
This builds a statically linked x86-64 binary against the **exact** glibc the
`libc-x86.LE.64` pack holds (`glibc-static 2.17-292.el7`, CentOS 7.7.1908), so
the libc objects in it are byte-identical to the ones the pack was hashed from.

```bash
docker build --load -t fid-el7 crates/fission-decompiler/examples/fid_static_fixture
docker run --rm -v "$PWD/crates/fission-decompiler/examples/fid_static_fixture:/w" fid-el7 \
  sh -c 'cd /w && gcc -std=gnu99 -static -O2 -o prog_static prog.c -lm'
cargo run --release -p fission-decompiler --example fid_precision -- \
  crates/fission-decompiler/examples/fid_static_fixture/prog_static
```

`fid_precision` runs the production `identify` path on the unstripped binary
and scores each name against the symbol table at that address (an alias such as
`regfree`/`__regfree` counts as correct). The binary is not committed.

Measured 2026-11-06 (1,558 functions, 1,447 hashed):

| configuration | identified | correct | wrong |
|---|---|---|---|
| flat +10 specific bonus (current) | 270 | 241 | 29 |
| Ghidra `scoreMatch` (0.67 x additional size, auto-pass floor 24, no cap) | 208 | 189 | 19 |
| current + relation scores always computed | 271 | 242 | 29 |
| Ghidra + relation scores always computed | 210 | 191 | 19 |
