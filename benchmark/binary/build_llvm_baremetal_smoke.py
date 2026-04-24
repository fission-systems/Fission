#!/usr/bin/env python3
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
BINARY_ROOT = ROOT / "benchmark" / "binary"
TEMPLATES_DIR = BINARY_ROOT / "templates"
SUMMARY_PATH = BINARY_ROOT / "llvm_baremetal_build_summary.json"


def spec(target: str, *flags: str) -> dict[str, object]:
    return {"target": target, "flags": list(flags)}


TARGET_SPECS: dict[str, dict[str, object]] = {
    "x86": spec("i686-none-elf", "-march=pentiumpro"),
    "x86-64": spec("x86_64-none-elf"),
    "AARCH64": spec("aarch64-none-elf", "-march=armv8-a"),
    "AARCH64BE": spec("aarch64_be-none-elf", "-march=armv8-a"),
    "AARCH64_AppleSilicon": spec("arm64-apple-macos11"),
    "ARM4_le": spec("arm-none-eabi", "-march=armv4"),
    "ARM4_be": spec("arm-none-eabi", "-march=armv4", "-mbig-endian"),
    "ARM4t_le": spec("arm-none-eabi", "-march=armv4t"),
    "ARM4t_be": spec("arm-none-eabi", "-march=armv4t", "-mbig-endian"),
    "ARM5_le": spec("arm-none-eabi", "-march=armv5te"),
    "ARM5_be": spec("arm-none-eabi", "-march=armv5te", "-mbig-endian"),
    "ARM5t_le": spec("arm-none-eabi", "-march=armv5t"),
    "ARM5t_be": spec("arm-none-eabi", "-march=armv5t", "-mbig-endian"),
    "ARM6_le": spec("arm-none-eabi", "-march=armv6"),
    "ARM6_be": spec("arm-none-eabi", "-march=armv6", "-mbig-endian"),
    "ARM7_le": spec("arm-none-eabi", "-march=armv7-a"),
    "ARM7_be": spec("arm-none-eabi", "-march=armv7-a", "-mbig-endian"),
    "ARM8_le": spec("arm-none-eabi", "-march=armv8-a"),
    "ARM8_be": spec("arm-none-eabi", "-march=armv8-a", "-mbig-endian"),
    "ARM8m_le": spec("arm-none-eabi", "-march=armv8-m.main"),
    "ARM8m_be": spec("arm-none-eabi", "-march=armv8-m.main", "-mbig-endian"),
    "mips32le": spec("mipsel-none-elf", "-march=mips32"),
    "mips32be": spec("mips-none-elf", "-march=mips32"),
    "mips32R6le": spec("mipsel-none-elf", "-march=mips32r6"),
    "mips32R6be": spec("mips-none-elf", "-march=mips32r6"),
    "mips64le": spec("mips64el-none-elf", "-march=mips64"),
    "mips64be": spec("mips64-none-elf", "-march=mips64"),
    "ppc_32_be": spec("powerpc-none-eabi"),
    "ppc_32_le": spec("powerpcle-none-eabi"),
    "ppc_32_4xx_be": spec("powerpc-none-eabi", "-mcpu=405"),
    "ppc_32_4xx_le": spec("powerpcle-none-eabi", "-mcpu=405"),
    "ppc_32_e500_be": spec("powerpc-none-eabi", "-mcpu=e500"),
    "ppc_32_e500_le": spec("powerpcle-none-eabi", "-mcpu=e500"),
    "ppc_32_e500mc_be": spec("powerpc-none-eabi", "-mcpu=e500mc"),
    "ppc_32_e500mc_le": spec("powerpcle-none-eabi", "-mcpu=e500mc"),
    "ppc_32_quicciii_be": spec("powerpc-none-eabi", "-mcpu=603e"),
    "ppc_32_quicciii_le": spec("powerpcle-none-eabi", "-mcpu=603e"),
    "ppc_64_be": spec("powerpc64-none-elf"),
    "ppc_64_le": spec("powerpc64le-none-elf"),
    "ppc_64_isa_be": spec("powerpc64-none-elf"),
    "ppc_64_isa_le": spec("powerpc64le-none-elf"),
    "ppc_64_isa_altivec_be": spec("powerpc64-none-elf", "-maltivec"),
    "ppc_64_isa_altivec_le": spec("powerpc64le-none-elf", "-maltivec"),
    "ppc_64_isa_vle_be": spec("powerpc64-none-elf", "-mvle"),
    "ppc_64_isa_altivec_vle_be": spec("powerpc64-none-elf", "-maltivec", "-mvle"),
    "riscv.ilp32d": spec("riscv32-none-elf", "-march=rv32imafdc", "-mabi=ilp32d"),
    "riscv.lp64d": spec("riscv64-none-elf", "-march=rv64imafdc", "-mabi=lp64d"),
    "loongarch32_f32": spec("loongarch32-none-elf", "-march=loongarch32", "-mabi=ilp32f"),
    "loongarch32_f64": spec("loongarch32-none-elf", "-march=loongarch32", "-mabi=ilp32d"),
    "loongarch64_f32": spec("loongarch64-none-elf", "-march=loongarch64", "-mabi=lp64f"),
    "loongarch64_f64": spec("loongarch64-none-elf", "-march=loongarch64", "-mabi=lp64d"),
    "SparcV9_32": spec("sparc-none-elf", "-march=v9"),
    "SparcV9_64": spec("sparcv9-none-elf"),
    "BPF_le": spec("bpfel"),
    "eBPF_le": spec("bpfel"),
    "eBPF_be": spec("bpfeb"),
}


def main() -> int:
    clang_c = shutil.which("clang")
    clang_cpp = shutil.which("clang++")
    if not clang_c or not clang_cpp:
        raise SystemExit("clang or clang++ not found in PATH")

    templates = list(TEMPLATES_DIR.glob("*.c")) + list(TEMPLATES_DIR.glob("*.cpp"))
    if not templates:
        raise SystemExit(f"No templates found in {TEMPLATES_DIR}")

    summary: dict[str, object] = {
        "builder": "clang",
        "templates_dir": str(TEMPLATES_DIR),
        "attempted": 0,
        "succeeded": [],
        "failed": [],
    }

    common_flags = [
        "-O2",
        "-ffreestanding",
        "-fno-builtin",
        "-fno-stack-protector",
        "-nostdlib",
        "-c",
    ]

    for entry_id in sorted(TARGET_SPECS):
        target = str(TARGET_SPECS[entry_id]["target"])
        extra_flags = list(TARGET_SPECS[entry_id]["flags"])
        for template in templates:
            lang = template.suffix[1:]
            source_dir = BINARY_ROOT / entry_id / "baremetal" / "small" / "source" / lang
            binary_dir = BINARY_ROOT / entry_id / "baremetal" / "small" / "binary" / lang
            source_dir.mkdir(parents=True, exist_ok=True)
            binary_dir.mkdir(parents=True, exist_ok=True)

            source_path = source_dir / template.name
            source_path.write_text(template.read_text())
            output_path = binary_dir / template.with_suffix(".o").name

            compiler = clang_cpp if lang == "cpp" else clang_c
            lang_flags = ["-fno-exceptions", "-fno-rtti"] if lang == "cpp" else []
            cmd = [compiler, f"--target={target}", *common_flags, *lang_flags, *extra_flags, str(source_path), "-o", str(output_path)]
            summary["attempted"] = int(summary["attempted"]) + 1
            result = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
            if result.returncode == 0 and output_path.exists():
                size = output_path.stat().st_size
                cast = summary["succeeded"]
                assert isinstance(cast, list)
                cast.append(
                    {
                        "entry_id": entry_id,
                        "template": template.name,
                        "target": target,
                        "flags": extra_flags,
                        "output": str(output_path.relative_to(ROOT)),
                        "size": size,
                    }
                )
                continue

            if output_path.exists():
                output_path.unlink()
            failed = summary["failed"]
            assert isinstance(failed, list)
            failed.append(
                {
                    "entry_id": entry_id,
                    "template": template.name,
                    "target": target,
                    "flags": extra_flags,
                    "stderr": result.stderr.strip(),
                    "stdout": result.stdout.strip(),
                    "returncode": result.returncode,
                }
            )

    SUMMARY_PATH.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            {
                "attempted": summary["attempted"],
                "succeeded": len(summary["succeeded"]),
                "failed": len(summary["failed"]),
                "summary": str(SUMMARY_PATH.relative_to(ROOT)),
            },
            indent=2,
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
