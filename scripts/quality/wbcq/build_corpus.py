#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Build the WBCQ P3 evaluation corpus (RFC #1093).

Compiles `corpus/*.c` for x86-64 and AArch64 Linux in four variants and links
each (ISA, variant) into one static ELF:

  O0          clang -O0
  O1          clang -O1 to IR, llc -O1
  O1-fla      as O1, IR flattened by `wbcq-fla` (after `lower-switch`)
  O1-bcf-fla  as O1-fla, with `wbcq-bcf` opaque-predicate branches first

The obfuscation passes are `obfuscator/WbcqObfuscator.cpp`, built here as an
`opt` plugin against the same LLVM. The IR is obfuscated *after* the -O1
pipeline and lowered with `llc` only, so no IR optimization runs over the
obfuscated code to undo it -- the order OLLVM uses.

Also builds the positive state machines natively as a shared library for
`ground_truth.py`. Outputs go under --out, outside the repository.

  python3 scripts/quality/wbcq/build_corpus.py --out ~/fission-benchmark/wbcq-p3/corpus
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import subprocess
from pathlib import Path

HERE = Path(__file__).resolve().parent
CORPUS = HERE / "corpus"
TARGETS = {"x86_64": "x86_64-linux-gnu", "aarch64": "aarch64-linux-gnu"}
VARIANTS = ("O0", "O1", "O1-fla", "O1-bcf-fla")
PASSES = {"O1-fla": "lower-switch,wbcq-fla", "O1-bcf-fla": "wbcq-bcf,lower-switch,wbcq-fla"}
# A leaf with no globals: no tables, no stack protector (it reads TLS), no PIC.
CFLAGS = ["-fno-jump-tables", "-fno-stack-protector", "-fno-pic", "-ffreestanding",
          "-fno-builtin", "-fno-unwind-tables", "-fno-asynchronous-unwind-tables"]
START = {"x86_64": ".globl _start\n_start: ud2\n", "aarch64": ".globl _start\n_start: brk #0\n"}


def run(*args: str | Path) -> None:
    subprocess.run([str(a) for a in args], check=True)


def tool(llvm: Path, name: str) -> Path:
    path = llvm / name
    if not path.exists():
        found = shutil.which(name)
        if not found:
            raise SystemExit(f"{name} not found in {llvm} or on PATH")
        path = Path(found)
    return path


def build_plugin(llvm: Path, out: Path) -> Path:
    config = tool(llvm, "llvm-config")
    flags = subprocess.check_output([str(config), "--cxxflags"], text=True).split()
    suffix = ".dylib" if platform.system() == "Darwin" else ".so"
    plugin = out / f"libWbcqObfuscator{suffix}"
    link = ["-undefined", "dynamic_lookup"] if platform.system() == "Darwin" else []
    run(tool(llvm, "clang++"), *flags, "-fPIC", "-shared", "-O2", *link, "-o", plugin,
        HERE / "obfuscator" / "WbcqObfuscator.cpp")
    return plugin


def build_variant(llvm: Path, plugin: Path, isa: str, variant: str, sources: list[Path],
                  out: Path) -> Path:
    triple = TARGETS[isa]
    work = out / isa / variant
    work.mkdir(parents=True, exist_ok=True)
    objects = []
    clang = tool(llvm, "clang")
    for source in sources:
        obj = work / f"{source.stem}.o"
        if variant == "O0":
            run(clang, f"--target={triple}", "-O0", *CFLAGS, "-c", source, "-o", obj)
        else:
            ir = work / f"{source.stem}.ll"
            run(clang, f"--target={triple}", "-O1", *CFLAGS, "-emit-llvm", "-S", source, "-o", ir)
            if variant in PASSES:
                obf = work / f"{source.stem}.obf.ll"
                run(tool(llvm, "opt"), "-load-pass-plugin", plugin, f"-passes={PASSES[variant]}",
                    "-S", ir, "-o", obf)
                run(tool(llvm, "opt"), "-passes=verify", "-disable-output", obf)
                ir = obf
            asm = work / f"{source.stem}.s"
            run(tool(llvm, "llc"), "-O1", f"-mtriple={triple}", "-relocation-model=static", ir,
                "-o", asm)
            run(clang, f"--target={triple}", "-c", asm, "-o", obj)
        objects.append(obj)
    start = work / "start.s"
    start.write_text(START[isa])
    run(clang, f"--target={triple}", "-c", start, "-o", work / "start.o")
    elf = work / "corpus.elf"
    run(tool(llvm, "ld.lld"), "-static", "-e", "_start", "--build-id=none",
        *objects, work / "start.o", "-o", elf)
    return elf


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--llvm-bin", type=Path, default=Path("/opt/homebrew/opt/llvm/bin"))
    args = parser.parse_args()
    out = args.out.expanduser().resolve()
    out.mkdir(parents=True, exist_ok=True)
    manifest = json.loads((CORPUS / "manifest.json").read_text())
    sources = sorted({CORPUS / entry["source"] for entry in manifest["functions"]})

    plugin = build_plugin(args.llvm_bin, out)
    built = {}
    for isa in TARGETS:
        for variant in VARIANTS:
            elf = build_variant(args.llvm_bin, plugin, isa, variant, sources, out)
            built[f"{isa}/{variant}"] = str(elf)
            print(f"{isa:8} {variant:11} {elf}")

    native = out / ("corpus_native.dylib" if platform.system() == "Darwin" else "corpus_native.so")
    run(tool(args.llvm_bin, "clang"), "-O1", "-shared", "-fPIC", "-o", native,
        CORPUS / "state_machines.c")
    (out / "manifest.json").write_text(json.dumps(
        {**manifest, "binaries": built, "native": str(native)}, indent=2) + "\n")
    print(f"native   {native}")


if __name__ == "__main__":
    main()
