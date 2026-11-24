"""Resolve an Actions event into the required CI jobs for its checked-out commit."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
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
# Files that change which advisories apply. Anything else leaves the advisory
# answer to the advisory database, which Heavy checks nightly.
DEPENDENCY_SET = {
    "Cargo.lock", "deny.toml", ".cargo/audit.toml", ".cargo/config", ".cargo/config.toml",
}
# The pre-merge lanes' own definitions. A change here is tested by running every
# lane it defines, not only by linting it: a lane nobody has seen run is unproven.
PREMERGE = (
    ".github/workflows/reusable-premerge.yml", "scripts/ci/decompiler_smoke.py",
    "scripts/macos/", ".github/fixtures/",
    ".github/workflows/reusable-base-head.yml", "scripts/ci/output_diff.py",
    "scripts/ci/speed_compare.py",
)
# What the native single-instruction differential checks: the SLEIGH lift
# (runtime and specs) and the p-code interpreter that executes it, with what
# the interpreter calls into -- the userop answers under `arch/` (vector ones
# included, a file or a directory), the shared float helpers and the
# CALLOTHER callbacks.
NATIVE_DIFF = (
    "crates/fission-sleigh/", "utils/sleigh-specs/", "crates/fission-emulator/src/pcode",
    "crates/fission-emulator/src/interp.rs", "crates/fission-emulator/src/arch/",
    "crates/fission-emulator/src/jit/float_ops.rs", "crates/fission-emulator/src/jit/softfloat.rs",
    "crates/fission-emulator/src/jit/callbacks.rs",
    "crates/fission-emulator/tests/native_differential/",
    ".github/workflows/native-differential.yml",
)
# The nightly full-dataset sweep cannot run on a PR; one binary through the same
# script proves the download, strip, decompile and scoring path still works.
SWEEP = (
    ".github/workflows/decbench-sweep.yml", ".github/workflows/reusable-decbench-sweep.yml",
    "scripts/quality/decbench_full_sweep.py",
    "scripts/quality/decbench_scoring.py", "scripts/quality/decbench_sample_set_gate.py",
    "scripts/quality/decbench-ci-requirements.txt",
)


DOC_CHECKS = (
    "docs/", "scripts/ci/check_docs.py", "scripts/ci/check_contract_schemas.py",
    "scripts/ci/doc-check-requirements.txt", "crates/fission-cli/", ".github/workflows/ci.yml",
)
DOC_ROOT = {"README.md", "CONTRIBUTING.md", "mkdocs.yml"}
REPLAY_CHECKS = (
    "crates/fission-dir/", "crates/fission-emulator/", "scripts/test/replay_fixture.py",
    ".github/workflows/ci.yml",
    "crates/fission-pcode", "crates/fission-midend-", "crates/fission-sleigh/",
    "crates/fission-decompiler/", "crates/fission-cspec/", "utils/sleigh-specs/",
)


def git(*args: str) -> bytes:
    return subprocess.check_output(["git", *args])


def revision(value: str) -> str:
    if not SHA.fullmatch(value) or value == "0" * 40:
        raise ValueError(f"Expected a commit SHA, got {value!r}")
    return git("rev-parse", "--verify", f"{value}^{{commit}}").decode().strip()


def is_ancestor(commit: str, descendant: str) -> bool:
    return subprocess.run(["git", "merge-base", "--is-ancestor", commit, descendant]).returncode == 0


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
        git("merge-base", "--is-ancestor", head, checkout)
        if not is_ancestor(base, checkout):
            # Retargeting a PR fires `edited` before GitHub recomputes the
            # merge ref, so the checkout can still be the merge with the old
            # base. The changed paths below come from base and head alone, so
            # the plan is still right; the merge queue tests the new base.
            print(
                f"::warning title=Stale merge ref::checkout {checkout[:12]} does not contain "
                f"base {base[:12]} yet; classifying {base[:12]}...{head[:12]}",
                file=sys.stderr,  # stdout is the step's $GITHUB_OUTPUT
            )
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
    # Cargo.lock can already contain a dependency newly enabled by a manifest.
    dependency_change = any(
        path in DEPENDENCY_SET or path.rsplit("/", 1)[-1] == "Cargo.toml" for path in paths
    )
    rust = False
    scripts = False
    workflows = False
    security = False
    for path in paths:
        if path in DOC_ROOT:
            continue
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
    lanes = before_merge and any(path.startswith(PREMERGE) for path in paths)
    native = full or lanes or any(path.startswith(DEBUGGER) for path in paths)
    return lane, {
        "run_doc_checks": event == "workflow_dispatch" or any(
            path in DOC_ROOT or path.startswith(DOC_CHECKS) for path in paths
        ),
        "run_replay_checks": event == "workflow_dispatch" or any(
            path.startswith(REPLAY_CHECKS) for path in paths
        ),
        "run_security": rust or security or dependency_change or event == "workflow_dispatch",
        # A push to main always compiles the whole workspace: two PRs that are
        # each green can still fail to build together, and a docs-only push is
        # the next chance to notice.
        "run_lint": rust or event == "push",
        "run_advisories": event == "workflow_dispatch" or dependency_change,
        "run_pass_gate": lane != "docs",
        "run_script_checks": workflows or scripts or security or event == "workflow_dispatch",
        "run_actionlint": workflows or event == "workflow_dispatch",
        "run_tests": rust or workflows,
        "run_debugger_tests": native,
        "run_cli_smoke": workflows,
        "run_macos_test": rust and not before_merge,
        "run_platform_debugger": native and before_merge,
        "run_solver_diff": lanes or before_merge and (full or any(path.startswith("crates/fission-solver/") for path in paths)),
        "run_native_diff": lanes or before_merge and (full or any(path.startswith(NATIVE_DIFF) for path in paths)),
        "run_loader_fixtures": lanes or before_merge and (full or any(path.startswith("crates/fission-loader/") for path in paths)),
        "run_decomp_corpus": lanes or before_merge and (full or any(path.startswith(DECOMPILER) for path in paths)),
        "run_package_check": before_merge and any(path.startswith(RELEASE) for path in paths),
        "run_sweep_smoke": before_merge and any(path.startswith(SWEEP) for path in paths),
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
