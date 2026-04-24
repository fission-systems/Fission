#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY_ROOT = ROOT / "benchmark" / "binary"
TEMPLATES_DIR = BINARY_ROOT / "templates"
SUMMARY_PATH = BINARY_ROOT / "go_linux_build_summary.json"

GOARCH_TARGETS = [
    "amd64",
    "386",
    "arm64",
    "arm",
    "mips64le",
    "mips64",
    "mipsle",
    "mips",
    "ppc64le",
    "riscv64",
]

def main() -> int:
    go = shutil.which("go")
    
    if not go:
        raise SystemExit("go not found in PATH")

    templates = list(TEMPLATES_DIR.glob("*.go"))
    if not templates:
        raise SystemExit(f"No go templates found in {TEMPLATES_DIR}")

    summary: dict[str, object] = {
        "builder": "go",
        "templates_dir": str(TEMPLATES_DIR),
        "attempted": 0,
        "succeeded": [],
        "failed": [],
    }

    env = os.environ.copy()
    env["CGO_ENABLED"] = "0"
    env["GOOS"] = "linux"

    for goarch in GOARCH_TARGETS:
        env["GOARCH"] = goarch
        
        entry_id = f"linux_{goarch}"
        source_dir = BINARY_ROOT / entry_id / "linux" / "small" / "source" / "go"
        binary_dir = BINARY_ROOT / entry_id / "linux" / "small" / "binary" / "go"
        source_dir.mkdir(parents=True, exist_ok=True)
        binary_dir.mkdir(parents=True, exist_ok=True)

        for template in templates:
            source_path = source_dir / template.name
            source_path.write_text(template.read_text())
            # Output is an executable, drop .go extension
            output_path = binary_dir / template.with_suffix("").name

            cmd = [go, "build", "-o", str(output_path), str(source_path)]
            summary["attempted"] = int(summary["attempted"]) + 1
            result = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
            
            if result.returncode == 0 and output_path.exists():
                size = output_path.stat().st_size
                cast = summary["succeeded"]
                assert isinstance(cast, list)
                cast.append(
                    {
                        "entry_id": entry_id,
                        "template": template.name,
                        "goos": "linux",
                        "goarch": goarch,
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
                    "goos": "linux",
                    "goarch": goarch,
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
