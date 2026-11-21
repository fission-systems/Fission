"""Resolve an Actions event into the required CI jobs for its checked-out commit."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from pathlib import Path

SHA = re.compile(r"[0-9a-f]{40}")
SHARED = {"Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml"}
DECOMPILER = (
    "crates/fission-pcode", "crates/fission-midend-", "crates/fission-sleigh/",
    "crates/fission-decompiler/", "crates/fission-cspec/", "utils/sleigh-specs/",
    "scripts/quality/", "benchmark/config/",
)
DEBUGGER = (
    "crates/fission-dynamic/", "crates/fission-loader/src/loader/elf/",
    "crates/fission-cli/src/cli/args/debug", "crates/fission-cli/src/cli/oneshot/debug",
    "crates/fission-cli/tests/debug_", "scripts/macos/",
)
RELEASE = (
    ".github/workflows/cd.yml", ".github/workflows/release-",
    "scripts/release/", "install.sh", ".github/scripts/check_decomp_output.py", ".github/actions/setup-utils/",
)
METADATA = {"deny.toml", "licenserc.yaml", ".github/dependabot.yml", ".github/licenserc.yaml"}


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args])


def revision(value: str) -> str:
    if not SHA.fullmatch(value) or value == "0" * 40:
        raise ValueError(f"Expected a commit SHA, got {value!r}")
    return git("rev-parse", "--verify", f"{value}^{{commit}}").decode().strip()


def event_paths(event: str, payload: dict, checkout: str) -> list[str]:
    checkout = revision(checkout)
    if git("rev-parse", "HEAD").decode().strip() != checkout:
        raise ValueError("checkout does not match GITHUB_SHA")
    if event == "workflow_dispatch":
        return ["Cargo.toml"]
    if event == "pull_request":
        pr = payload["pull_request"]
        base = revision(pr["base"]["sha"])
        head = revision(pr["head"]["sha"])
        # Actions runs the PR merge result. Classify branch changes against
        # the event's base, including when that base is another PR.
        for parent in (base, head):
            git("merge-base", "--is-ancestor", parent, checkout)
        base = git("merge-base", base, head).decode().strip()
    elif event == "merge_group":
        group = payload["merge_group"]
        base = revision(group["base_sha"])
        head = revision(group["head_sha"])
        if head != checkout:
            raise ValueError("merge-group head does not match checkout")
        git("merge-base", "--is-ancestor", base, head)
    elif event == "push":
        if payload["before"] == "0" * 40:
            return ["Cargo.toml"]
        base = revision(payload["before"])
        head = revision(payload["after"])
        if head != checkout:
            raise ValueError("push head does not match checkout")
    else:
        raise ValueError(f"Unexpected event {event!r}")
    # Treat renames as a removal and an addition, so both owners are tested.
    paths = git("diff", "--name-only", "--no-renames", "-z", base, head)
    names = [os.fsdecode(path) for path in paths.split(b"\0") if path]
    if any(not name.isprintable() or name != name.strip() for name in names):
        return ["Cargo.toml"]
    return names or ["Cargo.toml"]


def plan(paths: list[str], event: str) -> tuple[str, dict[str, bool]]:
    rust = False
    scripts = False
    workflows = False
    security = False
    for path in paths:
        if path in METADATA:
            security = True
        elif path.startswith((".github/workflows/", ".github/actions/", ".github/scripts/", ".github/actionlint.", "scripts/ci/")):
            workflows = True
        elif path.startswith(("scripts/", "benchmark/config/")):
            scripts = True
        elif path.startswith(("crates/", "utils/", ".cargo/")) or path in SHARED:
            rust = True
        elif not path.startswith(("docs/", "wiki/", ".github/ISSUE_TEMPLATE/")) and not path.endswith(".md"):
            rust = True
    lane = "rust" if rust else "ci" if workflows or security else "scripts" if scripts else "docs"
    full = any(path in SHARED or path.startswith(".cargo/") for path in paths)
    before_merge = event in {"pull_request", "merge_group"}
    native = full or any(path.startswith(DEBUGGER) for path in paths)
    return lane, {
        "run_security": rust or security,
        "run_lint": rust,
        "run_pass_gate": lane != "docs",
        "run_script_checks": workflows or scripts or security or event == "workflow_dispatch",
        "run_actionlint": workflows or event == "workflow_dispatch",
        "run_tests": rust or workflows,
        "run_debugger_tests": native,
        "run_cli_smoke": workflows,
        "run_macos_test": rust and not before_merge,
        "run_platform_debugger": native and before_merge,
        "run_solver_diff": before_merge and (full or any(path.startswith("crates/fission-solver/") for path in paths)),
        "run_loader_fixtures": before_merge and (full or any(path.startswith("crates/fission-loader/") for path in paths)),
        "run_decomp_corpus": before_merge and (full or any(path.startswith(DECOMPILER) for path in paths)),
        "run_package_check": before_merge and any(path.startswith(RELEASE) for path in paths),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--paths", required=True, type=Path)
    args = parser.parse_args()
    event = os.environ["GITHUB_EVENT_NAME"]
    payload = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
    paths = event_paths(event, payload, os.environ["GITHUB_SHA"])
    args.paths.write_text("\n".join(paths) + "\n")
    lane, jobs = plan(paths, event)
    print(f"lane={lane}")
    for name, enabled in jobs.items():
        print(f"{name}={str(enabled).lower()}")


if __name__ == "__main__":
    main()
