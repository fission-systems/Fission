#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Report CI defects to Sentry as closed-set events.

Reads what CI already writes -- nextest JUnit, the native differential log,
the fixture replay witness, libFuzzer crash files and `fission_cli verify`
JSON -- and turns each defect into one Sentry event under the shared defect
contract (docs/CI_RELEASE_GATES.md, "CI defect events"). Every value sent is
from a closed set or a pattern-checked identifier; a value that fails its
check is rejected with its event, never cleaned up. No message, panic text,
disassembly, decompiled output, symbol or input bytes is ever read into an
event: adapters look only at structure (element names, closed-set fields,
file names).

Sends only from push, merge_group, workflow_dispatch and schedule runs of
fission-systems/Fission, and only when SENTRY_DSN is set; otherwise it is a
dry run. Either way it writes a job summary of what was (or would be) sent,
and it always exits 0 once its arguments parse: reporting never changes a
job's result.

Usage (CI downloads its evidence artifacts into one directory first):
  sentry_defects.py --scan DIR [--dry-run] [--events-out FILE]
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
REPOSITORY = "fission-systems/Fission"
CLIENT = "fission-ci-defects/1"

# ── Contract (closed sets and patterns) ──────────────────────────────────────

KINDS = frozenset({
    "verify_diverged", "native_differential_mismatch", "fuzz_crash", "flaky_test", "test_failure",
})
ARCHES = frozenset({"x86", "x86_64", "aarch64", "arm", "thumb", "riscv64", "mips", "ppc", "unknown"})
STAGES = frozenset({
    "lift", "sleigh", "emulator", "normalize", "structuring", "render", "verify", "loader",
    "solver", "dir", "cli", "test", "unknown",
})
CATEGORIES = frozenset({"confirmed_semantic_defect", "crash", "flaky", "infra"})
LEVELS = {"flaky_test": "warning"}  # every other kind is "error"

COMPONENT_RE = re.compile(r"^(?:fission-[a-z0-9-]+|xtask)$")
INVARIANT_RE = re.compile(r"^[a-z0-9_.:-]{1,64}$")
REPRO_RE = re.compile(r"^[A-Za-z0-9_.:/@+-]{1,128}$")
# What a repro id may point at: an in-repo fixture (repository-relative path,
# optionally @ a hex address), a fuzz artifact, or a run's artifact.
REPRO_FORM_RE = re.compile(
    r"^(?:fixture:[A-Za-z0-9_.+-][A-Za-z0-9_./+-]*(?:@0x[0-9a-f]{1,16})?"
    r"|fuzz:fuzz_[a-z0-9_]{1,60}/(?:crash|leak|timeout|oom|slow-unit)-[0-9a-f]{40}"
    r"|run:[1-9][0-9]{0,19}/[A-Za-z0-9_.-]{1,100})$"
)
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
RUN_ID_RE = re.compile(r"^[1-9][0-9]{0,19}$")
RUN_URL_RE = re.compile(r"^https://github\.com/fission-systems/Fission/actions/runs/[1-9][0-9]{0,19}$")
RELEASE_RE = re.compile(r"^(?:fission-cli@[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.]+)?|fission@[0-9a-f]{12})$")
TAG_RE = re.compile(r"^v([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.]+)?)$")
ARTIFACT_RE = re.compile(r"^[A-Za-z0-9_.-]{1,100}$")

TAG_KEYS = ("kind", "component", "arch", "stage", "invariant", "category", "commit", "repro_id", "run_url")
DEFECT_KEYS = ("kind", "component", "arch", "stage", "invariant", "category", "repro_id")
EVENT_KEYS = frozenset({
    "event_id", "timestamp", "platform", "level", "release", "environment", "fingerprint", "tags",
    "exception",
})

MAX_EVENTS = 50
TRIAGE_ELIGIBLE = ("confirmed_semantic_defect", "crash")


class Rejected(ValueError):
    """A value failed its contract check; the event carrying it is dropped."""

    def __init__(self, field_name: str, reason: str):
        super().__init__(f"{field_name}: {reason}")
        self.field_name = field_name
        self.reason = reason


def check_tags(tags: dict) -> None:
    """Raise Rejected unless every tag is present and valid under the contract."""
    if set(tags) != set(TAG_KEYS):
        raise Rejected("tags", "tag set differs from the contract")
    _check_fields(tags, TAG_KEYS)


def _check_fields(tags: dict, keys) -> None:
    for key in keys:
        if not isinstance(tags.get(key), str):
            raise Rejected(key, "not a string")
    checks = (
        ("kind", tags.get("kind") in KINDS, "not a contract kind"),
        ("component", bool(COMPONENT_RE.fullmatch(tags.get("component", ""))), "not a Fission crate name"),
        ("arch", tags.get("arch") in ARCHES, "not a contract arch"),
        ("stage", tags.get("stage") in STAGES, "not a contract stage"),
        ("invariant", bool(INVARIANT_RE.fullmatch(tags.get("invariant", ""))), "fails ^[a-z0-9_.:-]{1,64}$"),
        ("category", tags.get("category") in CATEGORIES, "not a contract category"),
        ("commit", bool(COMMIT_RE.fullmatch(tags.get("commit", ""))), "not a 40-hex SHA"),
        ("repro_id", bool(REPRO_RE.fullmatch(tags.get("repro_id", ""))) and ".." not in tags.get("repro_id", "")
         and bool(REPRO_FORM_RE.fullmatch(tags.get("repro_id", ""))),
         "not a fixture:, fuzz: or run: id, fails the pattern, or contains '..'"),
        ("run_url", bool(RUN_URL_RE.fullmatch(tags.get("run_url", ""))), "not this repository's run URL"),
    )
    for key, ok, reason in checks:
        if key in keys and not ok:
            raise Rejected(key, reason)
    if (tags.get("kind") == "flaky_test") != (tags.get("category") == "flaky"):
        raise Rejected("category", "flaky_test and category flaky go together")


# ── Run context ──────────────────────────────────────────────────────────────

TRUSTED_EVENTS = {"push": "ci", "merge_group": "ci", "workflow_dispatch": "ci", "schedule": "nightly"}


@dataclass(frozen=True)
class RunContext:
    event_name: str
    repository: str
    sha: str
    run_id: str
    ref: str
    ref_type: str = "branch"
    ref_name: str = ""

    @classmethod
    def from_env(cls, env: dict) -> "RunContext":
        return cls(
            event_name=env.get("GITHUB_EVENT_NAME", ""),
            repository=env.get("GITHUB_REPOSITORY", ""),
            sha=env.get("GITHUB_SHA", ""),
            run_id=env.get("GITHUB_RUN_ID", ""),
            ref=env.get("GITHUB_REF", ""),
            ref_type=env.get("GITHUB_REF_TYPE", "branch"),
            ref_name=env.get("GITHUB_REF_NAME", ""),
        )

    def refusal(self) -> str | None:
        """Why events from this run must not be sent, or None if they may be."""
        if self.event_name not in TRUSTED_EVENTS:
            return f"event '{self.event_name or 'unknown'}' is not push, merge_group, workflow_dispatch or schedule"
        if self.repository != REPOSITORY:
            return "the run is not in fission-systems/Fission (a fork)"
        if not COMMIT_RE.fullmatch(self.sha):
            return "GITHUB_SHA is not a 40-hex SHA"
        if not RUN_ID_RE.fullmatch(self.run_id):
            return "GITHUB_RUN_ID is not a run id"
        if self.event_name == "merge_group":
            if not self.ref.startswith("refs/heads/gh-readonly-queue/main/"):
                return "a merge group not targeting main"
        elif self.ref != "refs/heads/main" and not (
            self.ref_type == "tag" and TAG_RE.fullmatch(self.ref_name)
        ):
            return "the run is not on main"
        return None

    @property
    def environment(self) -> str:
        return TRUSTED_EVENTS.get(self.event_name, "")

    @property
    def run_url(self) -> str:
        return f"https://github.com/{REPOSITORY}/actions/runs/{self.run_id}"

    @property
    def release(self) -> str:
        tag = TAG_RE.fullmatch(self.ref_name) if self.ref_type == "tag" else None
        return f"fission-cli@{tag.group(1)}" if tag else f"fission@{self.sha[:12]}"


# ── Defects and the summary ledger ───────────────────────────────────────────


@dataclass(frozen=True)
class Defect:
    kind: str
    component: str
    arch: str
    stage: str
    invariant: str
    category: str
    repro_id: str
    source: str  # which evidence file it came from (repository-side name), summary only

    def fingerprint(self) -> list[str]:
        base = [self.kind, self.component, self.stage, self.invariant]
        return base + [self.repro_id] if self.kind == "fuzz_crash" else base


@dataclass
class Ledger:
    """Everything an adapter saw, for the job summary."""

    defects: list[Defect] = field(default_factory=list)
    # (source, reason) for findings that are not defects under the contract.
    not_defects: list[tuple[str, str]] = field(default_factory=list)
    # (source, field, reason) for defects dropped because a value failed its check.
    rejected: list[tuple[str, str, str]] = field(default_factory=list)
    inputs: list[str] = field(default_factory=list)

    def extend(self, other: "Ledger") -> None:
        self.defects += other.defects
        self.not_defects += other.not_defects
        self.rejected += other.rejected
        self.inputs += other.inputs


def _add(ledger: Ledger, defect: Defect, ctx: RunContext) -> None:
    """Validate a defect's tags now so a bad value is reported against its source."""
    try:
        _check_fields(tags_for(defect, ctx), DEFECT_KEYS)
    except Rejected as err:
        ledger.rejected.append((defect.source, err.field_name, err.reason))
        return
    ledger.defects.append(defect)


def tags_for(defect: Defect, ctx: RunContext) -> dict:
    return {
        "kind": defect.kind,
        "component": defect.component,
        "arch": defect.arch,
        "stage": defect.stage,
        "invariant": defect.invariant,
        "category": defect.category,
        "commit": ctx.sha,
        "repro_id": defect.repro_id,
        "run_url": ctx.run_url,
    }


def build_event(defect: Defect, ctx: RunContext, *, event_id: str | None = None,
                timestamp: str | None = None) -> dict:
    """One Sentry event, validated; raises Rejected on any contract violation."""
    tags = tags_for(defect, ctx)
    check_tags(tags)
    if ctx.environment not in ("ci", "nightly"):
        raise Rejected("environment", "not ci or nightly")
    if not RELEASE_RE.fullmatch(ctx.release):
        raise Rejected("release", "not a contract release name")
    event = {
        "event_id": event_id or uuid.uuid4().hex,
        "timestamp": timestamp or _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "platform": "other",
        "level": LEVELS.get(defect.kind, "error"),
        "release": ctx.release,
        "environment": ctx.environment,
        "fingerprint": defect.fingerprint(),
        "tags": tags,
        "exception": {"values": [{
            "type": defect.kind,
            "value": f"{defect.kind} in {defect.component}/{defect.stage}",
        }]},
    }
    assert set(event) == EVENT_KEYS
    return event


def apply_caps(defects: list[Defect], limit: int = MAX_EVENTS) -> tuple[list[Defect], list[Defect], list[Defect]]:
    """(kept, duplicate fingerprints, over the run cap), deterministically ordered.

    Triage-eligible categories go first so a run that hits the cap keeps its
    defects and drops flaky reports.
    """
    order = {c: i for i, c in enumerate(("crash", "confirmed_semantic_defect", "infra", "flaky"))}
    ranked = sorted(defects, key=lambda d: (order.get(d.category, 9), d.fingerprint(), d.repro_id))
    seen: set[tuple[str, ...]] = set()
    unique: list[Defect] = []
    duplicates: list[Defect] = []
    for defect in ranked:
        key = tuple(defect.fingerprint())
        if key in seen:
            duplicates.append(defect)
        else:
            seen.add(key)
            unique.append(defect)
    return unique[:limit], duplicates, unique[limit:]


# ── Adapters ─────────────────────────────────────────────────────────────────


def _artifact_repro(ctx: RunContext, artifact: str) -> str:
    return f"run:{ctx.run_id}/{artifact}"


def _source(path: Path, base: Path | None) -> str:
    try:
        return str(path.relative_to(base)) if base else path.name
    except ValueError:
        return path.name


# nextest JUnit: <testsuite name="<binary id>"> holding <testcase name="<test path>">.
# A test that failed and then passed on a retry has <flakyFailure> children and
# no <failure>; one that failed every attempt has <failure> (+ <rerunFailure>).
# Only the failure `type` attribute is read: "test failure with exit code N",
# "test abort" (killed by a signal), "test timeout".

def test_invariant(binary_id: str, test_name: str) -> str:
    """The test's id as an invariant tag: lower case, shortened with a hash past 64 chars."""
    full = f"{binary_id}::{test_name}".lower()
    if len(full) > 64:
        digest = hashlib.sha256(full.encode()).hexdigest()[:8]
        full = f"{full[:55]}.{digest}"
    return full


def test_component(binary_id: str) -> str:
    return binary_id.split("::", 1)[0]


def parse_junit(path: Path, ctx: RunContext, artifact: str, base: Path | None = None) -> Ledger:
    ledger = Ledger(inputs=[_source(path, base)])
    source = _source(path, base)
    try:
        root = ET.parse(path).getroot()
    except (ET.ParseError, OSError):
        ledger.not_defects.append((source, "unreadable JUnit file (infra, not sent)"))
        return ledger
    for suite in root.iter("testsuite"):
        binary_id = suite.get("name", "")
        for case in suite.iter("testcase"):
            name = case.get("name", "")
            failure = case.find("failure")
            if failure is None:
                failure = case.find("error")
            flaky = case.find("flakyFailure")
            if failure is None and flaky is None:
                continue
            label = f"{source}: {binary_id} {name}"
            if failure is None:
                kind, category = "flaky_test", "flaky"
            else:
                kind_attr = failure.get("type", "")
                if kind_attr == "test abort":
                    kind, category = "test_failure", "crash"
                elif kind_attr == "test timeout":
                    ledger.not_defects.append((label, "test timeout (not a defect)"))
                    continue
                else:
                    # A deterministic failure already turns the job red and, on
                    # scheduled runs, opens a ci-red issue. Without a reviewed
                    # probe it is not a confirmed defect (docs/ISSUE_PR_WORKFLOW.md).
                    ledger.not_defects.append((label, "deterministic test failure (job status and ci-red report it)"))
                    continue
            _add(ledger, Defect(
                kind=kind,
                component=test_component(binary_id),
                arch="unknown",
                stage="test",
                invariant=test_invariant(binary_id, name),
                category=category,
                repro_id=_artifact_repro(ctx, artifact),
                source=label,
            ), ctx)
    return ledger


# Native differential (crates/fission-emulator/tests/native_differential): the
# gate's panic lists one line per mnemonic with fields no baseline entry
# explains. Stale baseline entries and empty filters also fail the gate but are
# not divergences.
NATIVE_HEADER_RE = re.compile(r"native differential \(([a-z0-9_]+)\) failed:")
NATIVE_NEW_RE = re.compile(r"^\s+([A-Za-z0-9_.]+): new divergence in \[(.*)\]\s*$")
NATIVE_STALE_RE = re.compile(r"^\s+baseline\.txt:\d+: .* no longer reproduces")
NATIVE_FIELD_RE = re.compile(r'^"([A-Za-z0-9_]+)"$')
NATIVE_ISAS = {"x86_64": "x86_64", "aarch64": "aarch64"}
MNEMONIC_RE = re.compile(r"^[a-z0-9_.]{1,32}$")


ANSI_RE = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")


def _strip_log_prefix(line: str) -> str:
    # `gh run view --log` prefixes "<job>\t<step>\t<timestamp> "; a tee'd log has none.
    line = ANSI_RE.sub("", line)
    if "\t" in line:
        line = line.rsplit("\t", 1)[1]
        line = re.sub(r"^\d{4}-\d\d-\d\dT[0-9:.]+Z ", "", line)
    return line


def parse_native_log(path: Path, ctx: RunContext, artifact: str, base: Path | None = None) -> Ledger:
    ledger = Ledger(inputs=[_source(path, base)])
    source = _source(path, base)
    isa = None
    try:
        lines = path.read_text(errors="replace").splitlines()
    except OSError:
        ledger.not_defects.append((source, "unreadable native differential log (infra, not sent)"))
        return ledger
    for raw in lines:
        line = _strip_log_prefix(raw)
        header = NATIVE_HEADER_RE.search(line)
        if header:
            isa = header.group(1)
            continue
        if isa is None:
            continue
        if NATIVE_STALE_RE.match(line):
            ledger.not_defects.append((f"{source} ({isa})", "stale baseline entry (a fix, not a defect)"))
            continue
        new = NATIVE_NEW_RE.match(line)
        if not new:
            if line.strip() and not line.startswith((" ", "\t")):
                isa = None  # the panic message ended
            continue
        mnemonic = new.group(1).lower()
        fields = [f.strip() for f in new.group(2).split(",") if f.strip()]
        label = f"{source} ({isa})"
        arch = NATIVE_ISAS.get(isa)
        if arch is None:
            ledger.rejected.append((label, "arch", "not a native differential ISA"))
            continue
        if not MNEMONIC_RE.fullmatch(mnemonic):
            ledger.rejected.append((label, "invariant", "mnemonic outside [a-z0-9_.]"))
            continue
        for quoted in fields:
            match = NATIVE_FIELD_RE.fullmatch(quoted)
            if not match:
                ledger.rejected.append((label, "invariant", "field name outside [A-Za-z0-9_]"))
                continue
            name = match.group(1).lower()
            # "panic" means Fission's side panicked on the instruction.
            category = "crash" if name == "panic" else "confirmed_semantic_defect"
            _add(ledger, Defect(
                kind="native_differential_mismatch",
                component="fission-emulator",
                arch=arch,
                stage="lift" if name == "decode" else "emulator",
                invariant=f"native.{arch}.{mnemonic}.{name}",
                category=category,
                repro_id=_artifact_repro(ctx, artifact),
                source=label,
            ), ctx)
    return ledger


# Fixture replay witness (scripts/test/replay_fixture.py): the native run is the
# ground truth; each other lane that disagrees is a confirmed defect.
REPLAY_LANES = {
    "emulator": ("fission-emulator", "emulator"),
    "hir": ("fission-dir", "verify"),
    "python": ("fission-dir", "dir"),
}
REPLAY_FIELDS = ("return_value", "buffer", "outside_writes")
REPLAY_ABIS = {"x86_64-linux-sysv": "x86_64"}


def parse_replay_witness(path: Path, ctx: RunContext, artifact: str, base: Path | None = None) -> Ledger:
    ledger = Ledger(inputs=[_source(path, base)])
    source = _source(path, base)
    try:
        witness = json.loads(path.read_text())
    except (OSError, ValueError):
        ledger.not_defects.append((source, "unreadable replay witness (infra, not sent)"))
        return ledger
    if not isinstance(witness, dict) or witness.get("version") != 1:
        ledger.not_defects.append((source, "unknown replay witness version (not sent)"))
        return ledger
    arch = REPLAY_ABIS.get(witness.get("abi"))
    if arch is None:
        ledger.rejected.append((source, "arch", "witness ABI is not a known fixture ABI"))
        return ledger
    observations = witness.get("observations")
    native = observations.get("native") if isinstance(observations, dict) else None
    if not isinstance(native, dict):
        ledger.not_defects.append((source, "witness has no native observation (not sent)"))
        return ledger
    for lane, (component, stage) in REPLAY_LANES.items():
        seen = observations.get(lane)
        if not isinstance(seen, dict):
            continue
        for name in REPLAY_FIELDS:
            if seen.get(name) != native.get(name):
                _add(ledger, Defect(
                    kind="verify_diverged",
                    component=component,
                    arch=arch,
                    stage=stage,
                    invariant=f"replay.{lane}.{name}",
                    category="confirmed_semantic_defect",
                    repro_id=_artifact_repro(ctx, artifact),
                    source=f"{source} ({lane})",
                ), ctx)
    return ledger


# libFuzzer writes crash-, leak-, timeout-, oom- and slow-unit-<sha1>. Only a
# crash (panic or deadly signal) is a defect here; file contents are never read.
FUZZ_FILE_RE = re.compile(r"^(crash|leak|timeout|oom|slow-unit)-([0-9a-f]{40})$")
FUZZ_TARGET_RE = re.compile(r"^fuzz_[a-z0-9_]{1,60}$")
FUZZ_STAGES = {"fission-loader": "loader", "fission-sleigh": "sleigh"}


def fuzz_target_owner(target: str, repo_root: Path = REPO_ROOT) -> str | None:
    """The crate whose fuzz/ directory defines `target`, from the checkout."""
    if not FUZZ_TARGET_RE.fullmatch(target):
        return None
    owners = sorted(p.parents[2].name for p in repo_root.glob(f"crates/*/fuzz/fuzz_targets/{target}.rs"))
    return owners[0] if len(owners) == 1 else None


def scan_fuzz_dir(directory: Path, target: str, ctx: RunContext, base: Path | None = None,
                  repo_root: Path = REPO_ROOT) -> Ledger:
    source = _source(directory, base)
    ledger = Ledger(inputs=[source])
    owner = fuzz_target_owner(target, repo_root)
    if owner is None:
        ledger.rejected.append((source, "component", "fuzz target is not defined by exactly one crate"))
        return ledger
    for entry in sorted(directory.iterdir()):
        if not entry.is_file():
            continue
        match = FUZZ_FILE_RE.fullmatch(entry.name)
        if not match:
            ledger.not_defects.append((f"{source}/{entry.name}", "not a libFuzzer artifact (ignored)"))
            continue
        if match.group(1) != "crash":
            ledger.not_defects.append((f"{source}/{entry.name}", f"libFuzzer {match.group(1)} (not a crash)"))
            continue
        _add(ledger, Defect(
            kind="fuzz_crash",
            component=owner,
            arch="unknown",
            stage=FUZZ_STAGES.get(owner, "unknown"),
            invariant=f"fuzz.{target}",
            category="crash",
            repro_id=f"fuzz:{target}/{entry.name}",
            source=f"{source}/{entry.name}",
        ), ctx)
    return ledger


# `fission_cli verify --json`: one object (single function, results under
# `tiers`) or one line per function (batch, results under each tier's name).
# Only `category` and the numeric results are read; `detail`, `reason` and
# `name` are free text about the analyzed program and are never touched.
VERIFY_TIERS = ("concrete", "ground_truth", "symbolic")
VERIFY_NOT_DEFECTS = {
    "equivalent": None,
    "suspect": "verify suspect (recovered prototype; not a defect)",
    "unsupported": "verify unsupported (not a defect)",
    "harness": "verify harness failure (infra, not sent)",
    "inconclusive": "verify inconclusive (not a defect)",
    "error": "decompile error during verify (not a verify defect)",
}
ADDRESS_RE = re.compile(r"^0x[0-9a-f]{1,16}$")


def in_repo_fixture(binary: str, repo_root: Path = REPO_ROOT) -> str | None:
    """`binary` as a repository-relative path if it is a file inside the checkout."""
    if not binary or ".." in Path(binary).parts:
        return None
    path = Path(binary)
    resolved = (path if path.is_absolute() else repo_root / path).resolve()
    try:
        relative = resolved.relative_to(repo_root.resolve())
    except ValueError:
        return None
    return relative.as_posix() if resolved.is_file() else None


def _verify_invariant(tier: str, result: dict) -> str:
    if tier == "ground_truth":
        found = result.get("counterexamples")
        found = [c for c in found if isinstance(c, dict)] if isinstance(found, list) else []
        # The printed HIR could not be evaluated where the machine returned.
        if any(c.get("hir_result") is None and c.get("emulator_result") is not None for c in found):
            return "ground_truth.hir_eval"
        if any(c.get("hir_result") != c.get("emulator_result") for c in found):
            return "ground_truth.return_value"
        if any(c.get("detail") is not None for c in found):
            return "ground_truth.side_effect"
        return "ground_truth.diverged"
    return {"concrete": "concrete.prehir_hir", "symbolic": "symbolic.counterexample"}[tier]


def parse_verify(path: Path, ctx: RunContext, binary: str, arch: str, address: str | None = None,
                 base: Path | None = None, repo_root: Path = REPO_ROOT) -> Ledger:
    source = _source(path, base)
    ledger = Ledger(inputs=[source])
    try:
        text = path.read_text()
    except OSError:
        ledger.not_defects.append((source, "unreadable verify output (infra, not sent)"))
        return ledger
    records = []
    try:
        records = [json.loads(text)]
    except ValueError:
        for line in text.splitlines():
            if line.strip():
                try:
                    records.append(json.loads(line))
                except ValueError:
                    ledger.not_defects.append((source, "a verify line is not JSON (ignored)"))
    fixture = in_repo_fixture(binary, repo_root)
    for record in records:
        if not isinstance(record, dict):
            continue
        tiers = record.get("tiers")
        if not isinstance(tiers, dict):
            tiers = {t: record[t] for t in VERIFY_TIERS if isinstance(record.get(t), dict)}
        if "decompile" in record and isinstance(record["decompile"], dict):
            tiers = {**tiers, "decompile": record["decompile"]}
        addr = record.get("address", address)
        for tier, result in sorted(tiers.items()):
            if not isinstance(result, dict):
                continue
            category = result.get("category")
            if category != "diverged" or tier not in VERIFY_TIERS:
                reason = VERIFY_NOT_DEFECTS.get(category, "unknown verify category (not sent)")
                if reason:
                    ledger.not_defects.append((f"{source} ({tier})", reason))
                continue
            label = f"{source} ({tier})"
            if fixture is None:
                ledger.rejected.append((label, "repro_id", "binary is not an in-repo fixture"))
                continue
            if not isinstance(addr, str) or not ADDRESS_RE.fullmatch(addr):
                ledger.rejected.append((label, "repro_id", "no hex function address"))
                continue
            _add(ledger, Defect(
                kind="verify_diverged",
                component="fission-dir",
                arch=arch,
                stage="verify",
                invariant=_verify_invariant(tier, result),
                category="confirmed_semantic_defect",
                repro_id=f"fixture:{fixture}@{addr}",
                source=label,
            ), ctx)
    return ledger


# ── Discovery over downloaded artifacts ──────────────────────────────────────
#
#   <dir>/<artifact>/**/*.xml                    nextest JUnit
#   <dir>/<artifact>/**/native-diff.log          native differential log
#   <dir>/<artifact>/**/replay-witness.json      fixture replay witness
#   <dir>/fuzz-crashes-<target>-<sha>/           libFuzzer artifacts
#   <dir>/<artifact>/**/*.verify.jsonl + .meta   `fission_cli verify` output and
#                                                {"binary": ..., "arch": ...}

FUZZ_ARTIFACT_RE = re.compile(r"^fuzz-crashes-(fuzz_[a-z0-9_]+)-[0-9a-f]{40}$")


def scan(directory: Path, ctx: RunContext) -> Ledger:
    ledger = Ledger()
    if not directory.is_dir():
        return ledger
    for artifact_dir in sorted(p for p in directory.iterdir() if p.is_dir()):
        artifact = artifact_dir.name
        if not ARTIFACT_RE.fullmatch(artifact):
            ledger.rejected.append((artifact[:40], "repro_id", "artifact name outside [A-Za-z0-9_.-]"))
            continue
        fuzz = FUZZ_ARTIFACT_RE.fullmatch(artifact)
        if fuzz:
            ledger.extend(scan_fuzz_dir(artifact_dir, fuzz.group(1), ctx, directory))
            continue
        for path in sorted(artifact_dir.rglob("*")):
            if not path.is_file():
                continue
            if path.suffix == ".xml":
                ledger.extend(parse_junit(path, ctx, artifact, directory))
            elif path.name == "native-diff.log":
                ledger.extend(parse_native_log(path, ctx, artifact, directory))
            elif path.name == "replay-witness.json":
                ledger.extend(parse_replay_witness(path, ctx, artifact, directory))
            elif path.name.endswith(".verify.jsonl"):
                meta_path = path.with_name(path.name[: -len(".jsonl")] + ".meta.json")
                try:
                    meta = json.loads(meta_path.read_text())
                except (OSError, ValueError):
                    ledger.rejected.append((_source(path, directory), "repro_id", "verify output has no .meta.json"))
                    continue
                arch = meta.get("arch", "unknown") if isinstance(meta, dict) else "unknown"
                binary = meta.get("binary", "") if isinstance(meta, dict) else ""
                ledger.extend(parse_verify(path, ctx, str(binary), str(arch), base=directory))
    return ledger


# ── Sending ──────────────────────────────────────────────────────────────────

DSN_RE = re.compile(r"^https://([0-9a-f]{32})@([A-Za-z0-9.-]+(?::[0-9]{1,5})?)(/[A-Za-z0-9_/-]*)?/([0-9]{1,20})$")


def envelope_target(dsn: str) -> tuple[str, str]:
    """(envelope URL, X-Sentry-Auth header) for a DSN; ValueError if it is not one."""
    match = DSN_RE.fullmatch(dsn.strip())
    if not match:
        raise ValueError("SENTRY_DSN is not an https Sentry DSN")
    key, host, prefix, project = match.groups()
    prefix = (prefix or "").rstrip("/")
    url = f"https://{host}{prefix}/api/{project}/envelope/"
    auth = f"Sentry sentry_version=7, sentry_client={CLIENT}, sentry_key={key}"
    return url, auth


def envelope(event: dict) -> bytes:
    payload = json.dumps(event, separators=(",", ":"), sort_keys=True).encode()
    header = json.dumps({"event_id": event["event_id"], "sent_at": event["timestamp"]}).encode()
    item = json.dumps({"type": "event", "length": len(payload), "content_type": "application/json"}).encode()
    return header + b"\n" + item + b"\n" + payload + b"\n"


def send(events: list[dict], dsn: str, opener=None) -> list[tuple[str, str]]:
    """Send each event; [(event_id, outcome)]. Never raises for network errors."""
    url, auth = envelope_target(dsn)
    opener = opener or urllib.request.urlopen
    outcomes = []
    for i, event in enumerate(events):
        request = urllib.request.Request(url, data=envelope(event), method="POST", headers={
            "Content-Type": "application/x-sentry-envelope",
            "X-Sentry-Auth": auth,
            "User-Agent": CLIENT,
        })
        try:
            with opener(request, timeout=15) as response:
                outcomes.append((event["event_id"], f"sent (HTTP {response.status})"))
        except urllib.error.HTTPError as err:
            outcomes.append((event["event_id"], f"failed (HTTP {err.code})"))
            if err.code == 429:
                outcomes += [(e["event_id"], "not sent (rate limited)") for e in events[i + 1:]]
                break
        except (urllib.error.URLError, OSError) as err:
            outcomes.append((event["event_id"], f"failed ({type(err).__name__})"))
    return outcomes


# ── Summary ──────────────────────────────────────────────────────────────────


def _md(text: str) -> str:
    return text.replace("|", "\\|").replace("\n", " ")


def summary(ledger: Ledger, events: list[dict], duplicates: list[Defect], over_cap: list[Defect],
            mode: str, outcomes: dict[str, str]) -> str:
    out = ["## Sentry defect events", "", f"Mode: **{mode}**.", ""]
    if ledger.inputs:
        out.append(f"Evidence read: {len(ledger.inputs)} file(s).")
    else:
        out.append("No evidence files were found.")
    out.append("")
    verb = "Sent" if mode == "send" else "Would send"
    out.append(f"### {verb}: {len(events)} event(s)")
    if events:
        out += ["", "| level | kind | component | stage | arch | invariant | category | repro_id | outcome |",
                "|---|---|---|---|---|---|---|---|---|"]
        for event in events:
            t = event["tags"]
            out.append("| " + " | ".join(_md(x) for x in (
                event["level"], t["kind"], t["component"], t["stage"], t["arch"], t["invariant"],
                t["category"], t["repro_id"], outcomes.get(event["event_id"], "dry run"),
            )) + " |")
    for title, items in (("Same fingerprint as an event above (not sent)", duplicates),
                         (f"Over the {MAX_EVENTS}-event run cap (not sent)", over_cap)):
        if items:
            out += ["", f"### {title}: {len(items)}", ""]
            out += [f"- `{d.kind}` `{d.component}` `{d.invariant}` ({_md(d.source)})" for d in items]
    if ledger.not_defects:
        out += ["", f"### Not defects under the contract (not sent): {len(ledger.not_defects)}", ""]
        out += [f"- {_md(src)}: {_md(reason)}" for src, reason in ledger.not_defects]
    if ledger.rejected:
        out += ["", f"### Rejected by validation (not sent): {len(ledger.rejected)}", ""]
        out += [f"- {_md(src)}: `{field_name}` {_md(reason)}" for src, field_name, reason in ledger.rejected]
    return "\n".join(out) + "\n"


def run(args: argparse.Namespace, env: dict, opener=None) -> tuple[str, list[dict]]:
    ctx = RunContext.from_env(env)
    ledger = Ledger()
    for directory in args.scan:
        ledger.extend(scan(Path(directory), ctx))
    for path in args.junit:
        ledger.extend(parse_junit(Path(path), ctx, args.artifact))
    for path in args.native_log:
        ledger.extend(parse_native_log(Path(path), ctx, args.artifact))
    for path in args.replay_witness:
        ledger.extend(parse_replay_witness(Path(path), ctx, args.artifact))
    for spec in args.fuzz_dir:
        target, _, directory = spec.partition("=")
        ledger.extend(scan_fuzz_dir(Path(directory), target, ctx))
    for path in args.verify_json:
        ledger.extend(parse_verify(Path(path), ctx, args.verify_binary, args.verify_arch, args.verify_addr))

    refusal = ctx.refusal()
    dsn = env.get("SENTRY_DSN", "").strip()
    if refusal:
        mode = f"dry run: not sending, {refusal}"
    elif args.dry_run:
        mode = "dry run (--dry-run)"
    elif not dsn:
        mode = "dry run: SENTRY_DSN is not set"
    else:
        mode = "send"

    kept, duplicates, over_cap = apply_caps(ledger.defects)
    events = []
    if not refusal:
        for defect in kept:
            try:
                events.append(build_event(defect, ctx))
            except Rejected as err:
                ledger.rejected.append((defect.source, err.field_name, err.reason))
    outcomes: dict[str, str] = {}
    if mode == "send" and events:
        try:
            outcomes = dict(send(events, dsn, opener))
        except ValueError as err:
            mode = f"dry run: {err}"
    if refusal and kept:
        # Show what the run found without building events it may not send.
        ledger.not_defects += [(d.source, f"found {d.kind}; not sent from this run") for d in kept]
    return summary(ledger, events, duplicates, over_cap, mode, outcomes), events


def main(argv: list[str] | None = None, env: dict | None = None, opener=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--scan", action="append", default=[], help="directory of downloaded evidence artifacts")
    parser.add_argument("--junit", action="append", default=[], help="nextest JUnit file")
    parser.add_argument("--native-log", action="append", default=[], help="native differential test output")
    parser.add_argument("--replay-witness", action="append", default=[], help="replay_fixture.py witness")
    parser.add_argument("--fuzz-dir", action="append", default=[], metavar="TARGET=DIR",
                        help="libFuzzer artifact directory of one target")
    parser.add_argument("--verify-json", action="append", default=[], help="fission_cli verify --json output")
    parser.add_argument("--verify-binary", default="", help="in-repo fixture the verify output is about")
    parser.add_argument("--verify-arch", default="unknown", help="that fixture's architecture (contract arch)")
    parser.add_argument("--verify-addr", default=None, help="function address for single-function verify output")
    parser.add_argument("--artifact", default="local", help="artifact name for repro ids of explicit inputs")
    parser.add_argument("--dry-run", action="store_true", help="never send, even with SENTRY_DSN set")
    parser.add_argument("--events-out", help="write the (would-be) events as JSON lines here")
    args = parser.parse_args(argv)
    env = dict(os.environ) if env is None else env
    try:
        text, events = run(args, env, opener)
    except Exception as err:  # noqa: BLE001 -- reporting must never fail a job
        text, events = f"## Sentry defect events\n\nReporter error: {type(err).__name__}; nothing sent.\n", []
    summary_path = env.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with open(summary_path, "a", encoding="utf-8") as stream:
            stream.write(text)
    print(text)
    if args.events_out:
        with open(args.events_out, "w", encoding="utf-8") as stream:
            for event in events:
                stream.write(json.dumps(event, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
