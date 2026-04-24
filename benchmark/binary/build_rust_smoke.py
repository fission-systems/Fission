#!/usr/bin/env python3
from __future__ import annotations

import json
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY_ROOT = ROOT / "benchmark" / "binary"
TEMPLATES_DIR = BINARY_ROOT / "templates"
SUMMARY_PATH = BINARY_ROOT / "rust_baremetal_build_summary.json"

TARGET_SPECS = [
    "aarch64-unknown-none",
    "x86_64-unknown-none",
    "riscv64imac-unknown-none-elf",
    "armv7a-none-eabi",
    "wasm32-unknown-unknown",
]

def main() -> int:
    rustc = shutil.which("rustc")
    rustup = shutil.which("rustup")
    
    if not rustc:
        raise SystemExit("rustc not found in PATH")

    if rustup:
        print("Installing rust targets...")
        for target in TARGET_SPECS:
            subprocess.run([rustup, "target", "add", target], check=False)

    templates = list(TEMPLATES_DIR.glob("*.rs"))
    if not templates:
        raise SystemExit(f"No rust templates found in {TEMPLATES_DIR}")

    summary: dict[str, object] = {
        "builder": "rustc",
        "templates_dir": str(TEMPLATES_DIR),
        "attempted": 0,
        "succeeded": [],
        "failed": [],
    }

    common_flags = [
        "-C", "opt-level=2",
        "--emit=obj",
    ]

    for target in TARGET_SPECS:
        # Fission uses LLVM target names for folders, but we'll use rustc target names here
        # or map them roughly if we wanted. For now, use the rustc target name as entry_id
        entry_id = target
        
        source_dir = BINARY_ROOT / entry_id / "baremetal" / "small" / "source" / "rust"
        binary_dir = BINARY_ROOT / entry_id / "baremetal" / "small" / "binary" / "rust"
        source_dir.mkdir(parents=True, exist_ok=True)
        binary_dir.mkdir(parents=True, exist_ok=True)

        for template in templates:
            source_path = source_dir / template.name
            source_path.write_text(template.read_text())
            output_path = binary_dir / template.with_suffix(".o").name

            cmd = [rustc, f"--target={target}", *common_flags, str(source_path), "-o", str(output_path)]
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
