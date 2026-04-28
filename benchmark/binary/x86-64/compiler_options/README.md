# x86-64 Compiler Option Matrix

This corpus intentionally builds the same C source across compiler and
optimization options. It is meant to expose SLEIGH constructor/template coverage
differences that do not show up in a single canonical x86-64 row set.

Build:

```bash
python3 benchmark/binary/build_x8664_option_matrix.py
```

Generated inputs:

- source: `benchmark/binary/x86-64/compiler_options/small/source/c/sleigh_option_matrix.c`
- binaries: `benchmark/binary/x86-64/compiler_options/small/binary/c/**/`
- build summary: `benchmark/binary/x86_64_compiler_option_matrix_summary.json`
- benchmark corpus: `benchmark/config/benchmark_corpus/x86_64_compiler_option_matrix.json`

The benchmark corpus includes loader-ready PE executables, ELF objects, and
standalone COFF objects when the local toolchain supports them.
