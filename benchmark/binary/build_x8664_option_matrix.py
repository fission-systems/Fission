#!/usr/bin/env python3
"""Build x86-64 compiler/option sample binaries for SLEIGH parity coverage."""

from __future__ import annotations

import json
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BINARY_ROOT = ROOT / "benchmark" / "binary"
SOURCE = BINARY_ROOT / "x86-64" / "compiler_options" / "small" / "source" / "c" / "sleigh_option_matrix.c"
OUT_ROOT = BINARY_ROOT / "x86-64" / "compiler_options" / "small" / "binary" / "c"
SUMMARY_PATH = BINARY_ROOT / "x86_64_compiler_option_matrix_summary.json"
CORPUS_PATH = ROOT / "benchmark" / "config" / "benchmark_corpus" / "x86_64_compiler_option_matrix.json"


@dataclass(frozen=True)
class BuildSpec:
    compiler_id: str
    compiler: str
    output_kind: str
    extension: str
    flags: tuple[str, ...]
    tags: tuple[str, ...]


def tool(path_or_name: str) -> str | None:
    return shutil.which(path_or_name)


def build_specs() -> list[BuildSpec]:
    return [
        BuildSpec("mingw-gcc-o0", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-O0", "-g", "-m64", "-static"), ("mingw-gcc", "O0", "pe")),
        BuildSpec("mingw-gcc-o1", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-O1", "-g", "-m64", "-static"), ("mingw-gcc", "O1", "pe")),
        BuildSpec("mingw-gcc-o2", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-O2", "-g", "-m64", "-static"), ("mingw-gcc", "O2", "pe")),
        BuildSpec("mingw-gcc-o3", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-O3", "-g", "-m64", "-static"), ("mingw-gcc", "O3", "pe")),
        BuildSpec("mingw-gcc-os", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-Os", "-g", "-m64", "-static"), ("mingw-gcc", "Os", "pe")),
        BuildSpec("mingw-gcc-og", "x86_64-w64-mingw32-gcc", "pe-exe", ".exe", ("-Og", "-g", "-m64", "-static"), ("mingw-gcc", "Og", "pe")),
        BuildSpec(
            "mingw-gcc-o2-frameptr",
            "x86_64-w64-mingw32-gcc",
            "pe-exe",
            ".exe",
            ("-O2", "-g", "-m64", "-static", "-fno-omit-frame-pointer"),
            ("mingw-gcc", "O2", "frame-pointer", "pe"),
        ),
        BuildSpec(
            "mingw-gcc-o2-noinline",
            "x86_64-w64-mingw32-gcc",
            "pe-exe",
            ".exe",
            ("-O2", "-g", "-m64", "-static", "-fno-inline"),
            ("mingw-gcc", "O2", "no-inline", "pe"),
        ),
        BuildSpec(
            "clang-elf-o0",
            "clang",
            "elf-object",
            ".o",
            ("--target=x86_64-none-elf", "-O0", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "O0", "elf", "freestanding"),
        ),
        BuildSpec(
            "clang-elf-o2",
            "clang",
            "elf-object",
            ".o",
            ("--target=x86_64-none-elf", "-O2", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "O2", "elf", "freestanding"),
        ),
        BuildSpec(
            "clang-elf-o3",
            "clang",
            "elf-object",
            ".o",
            ("--target=x86_64-none-elf", "-O3", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "O3", "elf", "freestanding"),
        ),
        BuildSpec(
            "clang-elf-os",
            "clang",
            "elf-object",
            ".o",
            ("--target=x86_64-none-elf", "-Os", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "Os", "elf", "freestanding"),
        ),
        BuildSpec(
            "clang-elf-o2-frameptr",
            "clang",
            "elf-object",
            ".o",
            ("--target=x86_64-none-elf", "-O2", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-fno-omit-frame-pointer", "-c"),
            ("clang", "O2", "frame-pointer", "elf", "freestanding"),
        ),
        BuildSpec(
            "clang-coff-o2",
            "clang",
            "coff-object",
            ".obj",
            ("--target=x86_64-w64-windows-gnu", "-O2", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "O2", "coff", "freestanding"),
        ),
        BuildSpec(
            "clang-coff-o3",
            "clang",
            "coff-object",
            ".obj",
            ("--target=x86_64-w64-windows-gnu", "-O3", "-g", "-ffreestanding", "-fno-builtin", "-fno-stack-protector", "-c"),
            ("clang", "O3", "coff", "freestanding"),
        ),
    ]


def run_build(spec: BuildSpec) -> dict[str, object]:
    compiler = tool(spec.compiler)
    output_dir = OUT_ROOT / spec.compiler_id
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"sleigh_option_matrix{spec.extension}"
    if output.exists():
        output.unlink()

    if compiler is None:
        return {
            "compiler_id": spec.compiler_id,
            "compiler": spec.compiler,
            "status": "skipped",
            "reason": "compiler_not_found",
        }

    cmd = [compiler, *spec.flags, str(SOURCE), "-o", str(output)]
    result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0 or not output.exists():
        return {
            "compiler_id": spec.compiler_id,
            "compiler": spec.compiler,
            "status": "failed",
            "returncode": result.returncode,
            "stdout": result.stdout.strip(),
            "stderr": result.stderr.strip(),
            "cmd": cmd,
        }

    return {
        "compiler_id": spec.compiler_id,
        "compiler": spec.compiler,
        "output_kind": spec.output_kind,
        "output": str(output.relative_to(ROOT)),
        "size": output.stat().st_size,
        "flags": list(spec.flags),
        "tags": list(spec.tags),
        "status": "succeeded",
        "cmd": cmd,
    }


def write_corpus(successes: list[dict[str, object]]) -> None:
    entries = []
    for row in successes:
        compiler_id = str(row["compiler_id"])
        output_kind = str(row["output_kind"])
        binary_path = ROOT / str(row["output"])
        tags = ["x86-64", "compiler-option-matrix", "small-c", output_kind, *list(row["tags"])]
        entries.append(
            {
                "id": f"x86-64-option-matrix-{compiler_id}",
                "binary_path": str(binary_path),
                "ghidra_project_key": f"x86_64_option_matrix_{compiler_id.replace('-', '_')}",
                "tags": tags,
                "seed_limit": 25,
                "role": "compiler_option_matrix",
                "weight": 1,
                "build": {
                    "compiler_id": compiler_id,
                    "compiler": row["compiler"],
                    "flags": row["flags"],
                    "output_kind": output_kind,
                },
            }
        )

    corpus = {
        "name": "x86-64-compiler-option-matrix",
        "suite_tier": "parity",
        "gate_mode": "advisory",
        "dynamic_watchlist_limit": 5,
        "notes": (
            "Locally generated x86-64 C samples built across compiler and optimization "
            "options to expose SLEIGH constructor/template coverage variation. "
            "PE executables, ELF objects, and standalone COFF objects are included "
            "when the local toolchain can build them."
        ),
        "entries": entries,
    }
    CORPUS_PATH.write_text(json.dumps(corpus, indent=2, sort_keys=True) + "\n")


def main() -> int:
    if not SOURCE.exists():
        raise SystemExit(f"source not found: {SOURCE}")

    results = [run_build(spec) for spec in build_specs()]
    successes = [row for row in results if row.get("status") == "succeeded"]
    write_corpus(successes)

    summary = {
        "source": str(SOURCE.relative_to(ROOT)),
        "output_root": str(OUT_ROOT.relative_to(ROOT)),
        "corpus": str(CORPUS_PATH.relative_to(ROOT)),
        "attempted": len(results),
        "succeeded": len(successes),
        "failed": len([row for row in results if row.get("status") == "failed"]),
        "skipped": len([row for row in results if row.get("status") == "skipped"]),
        "results": results,
    }
    SUMMARY_PATH.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(json.dumps({k: summary[k] for k in ("attempted", "succeeded", "failed", "skipped", "corpus")}, indent=2, sort_keys=True))
    return 0 if successes else 1


if __name__ == "__main__":
    raise SystemExit(main())
