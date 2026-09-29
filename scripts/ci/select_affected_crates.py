#!/usr/bin/env python3
"""Select Rust packages whose tests can be affected by changed paths."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
CORE_FALLBACK = {
    "fission-analysis-db",
    "fission-loader",
    "fission-midend-core",
    "fission-midend-normalize",
    "fission-midend-structuring",
    "fission-pcode",
}
WORKSPACE_WIDE_PATHS = {
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain",
    "rust-toolchain.toml",
}


def affected_packages(changed_paths: list[str]) -> set[str]:
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
            cwd=ROOT,
            text=True,
        )
    )
    packages = {package["name"]: package for package in metadata["packages"]}
    manifest_dirs = {
        Path(package["manifest_path"]).parent.relative_to(ROOT).as_posix(): name
        for name, package in packages.items()
    }

    reverse_dependencies = {name: set() for name in packages}
    for package in packages.values():
        for dependency in package["dependencies"]:
            dependency_name = dependency["name"]
            if dependency_name in packages:
                reverse_dependencies[dependency_name].add(package["name"])

    owners: set[str] = set()
    full_workspace = False

    for raw_path in changed_paths:
        path = raw_path.strip().removeprefix("./")
        if not path:
            continue

        if path in WORKSPACE_WIDE_PATHS or path.startswith(".cargo/"):
            full_workspace = True
            continue

        matching_dirs = [
            directory
            for directory in manifest_dirs
            if path == directory or path.startswith(directory + "/")
        ]
        if matching_dirs:
            owner_dir = max(matching_dirs, key=len)
            owners.add(manifest_dirs[owner_dir])
            continue

        if path.startswith("utils/sleigh-specs/"):
            owners.add("fission-sleigh")
        elif path.startswith("utils/signatures/"):
            owners.update(
                name
                for name in ("fission-core", "fission-loader", "fission-signatures", "fission-static")
                if name in packages
            )
        elif path.startswith(("utils/", "crates/")):
            # Unknown checked-in resources or a deleted/new crate need the broad
            # workspace lane until an owner mapping is added here.
            full_workspace = True
        elif path.startswith((".github/", "docs/", "wiki/", "scripts/", "benchmark/", "vendor/")) or path.endswith(".md"):
            continue
        else:
            # Unknown paths are rust-adjacent in the lane classifier. Preserve
            # that fail-safe behavior by running every workspace package.
            full_workspace = True

    if full_workspace:
        selected = set(packages)
    elif owners:
        selected = set(owners)
        pending = list(owners)
        while pending:
            dependency = pending.pop()
            for dependent in reverse_dependencies[dependency] - selected:
                selected.add(dependent)
                pending.append(dependent)
    else:
        # A Rust lane without a crate or resource owner retains the previous
        # core package floor. Unknown paths are handled by full_workspace.
        selected = CORE_FALLBACK & set(packages)

    return selected


def main() -> int:
    changed_paths = sys.stdin.read().splitlines()
    selected = affected_packages(changed_paths)
    ordered = sorted(selected)
    print(f"crates={','.join(ordered)}")
    print(f"run_cli_smoke={'true' if 'fission-cli' in selected else 'false'}")
    print(
        "run_interactive_runtime_tests="
        f"{'true' if 'fission-dynamic' in selected else 'false'}"
    )
    print(
        "run_script_emulator_tests="
        f"{'true' if 'fission-script' in selected else 'false'}"
    )
    print(
        "run_emulator_softfloat_check="
        f"{'true' if 'fission-emulator' in selected else 'false'}"
    )
    print(
        "run_plugin_interactive_check="
        f"{'true' if 'fission-plugin' in selected else 'false'}"
    )
    print(
        "run_cli_alt_feature_checks="
        f"{'true' if 'fission-cli' in selected else 'false'}"
    )
    print(f"Selected affected packages: {', '.join(ordered)}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
