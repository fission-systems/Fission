#!/usr/bin/env python3
"""Check CSIC's input-level verdicts by running the fixtures natively.

The emulator and the decompiler share Fission's SLEIGH lift, so agreement
between them cannot catch a lift error (ADR 0017). This script runs the
fixture binaries on a real Linux kernel instead: each case's recorded input,
every input-byte counterfactual witness the evaluation reported, every joint
pair, and a sample of bytes CSIC tested and found not to be causes. Native Φ
is read from stdout and the exit status (each fixture prints its sink).

Native here means Docker on the host: AArch64 binaries run on the host CPU
in the Linux VM; x86-64 binaries run under Docker's x86-64 translation
(Rosetta 2 / qemu), which is independent of Fission's SLEIGH lift but is not
bare x86 hardware.

Usage:
    verify_native.py REPORT.json [--fixtures DIR]

REPORT.json is the `--out` file of `examples/csic_eval.rs`. Images are built
locally from the fixtures with `docker import` (no registry pull).
"""

import argparse
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import time

# Native observable for Φ, per case: a function of (stdout, exit status).
PHI = {
    "rfc_auth_x64_O0": lambda out, rc: b"ADMIN" in out,
    "rfc_auth_x64_O2": lambda out, rc: b"ADMIN" in out,
    "rfc_auth_a64_O0": lambda out, rc: b"ADMIN" in out,
    "invalid_interventions": lambda out, rc: b"ADMIN" in out,
    "implicit_call_arg": lambda out, rc: b"GRANT7" in out,
    "false_alias_ptr": lambda out, rc: out.startswith(b"FLAG"),
    "real_alias": lambda out, rc: b"ADMIN" in out,
    "joint_or": lambda out, rc: b"GRANT" in out,
    "joint_and": lambda out, rc: b"GRANT" in out,
    "nondet_time": lambda out, rc: b"GRANT" in out,
    "nondet_random": lambda out, rc: b"GRANT" in out,
    "crash_null_x64": lambda out, rc: rc in (139, -11),
    "crash_null_a64": lambda out, rc: rc in (139, -11),
    "password_x64": lambda out, rc: b"ADMIN" in out,
    "password_a64": lambda out, rc: b"ADMIN" in out,
    "branch_len": lambda out, rc: b"LONG" in out,
}


def platform_of(fixture):
    return "linux/arm64" if fixture.startswith("a64_") else "linux/amd64"


def build_images(fixtures):
    with tempfile.TemporaryDirectory() as tmp:
        tar_path = os.path.join(tmp, "fixtures.tar")
        with tarfile.open(tar_path, "w") as tar:
            for name in sorted(os.listdir(fixtures)):
                if name.endswith(".elf"):
                    info = tar.gettarinfo(os.path.join(fixtures, name), arcname=name)
                    info.mode = 0o755
                    with open(os.path.join(fixtures, name), "rb") as f:
                        tar.addfile(info, f)
        for plat, tag in (("linux/arm64", "arm64"), ("linux/amd64", "amd64")):
            subprocess.run(
                ["docker", "import", "--platform", plat, tar_path, f"csic-fixtures:{tag}"],
                check=True,
                capture_output=True,
            )


def run_native(fixture, data):
    plat = platform_of(fixture)
    tag = "arm64" if plat.endswith("arm64") else "amd64"
    proc = subprocess.run(
        ["docker", "run", "--rm", "-i", "--platform", plat, f"csic-fixtures:{tag}", "/" + fixture],
        input=data,
        capture_output=True,
        timeout=60,
    )
    return proc.stdout, proc.returncode


def apply_inputs(data, edits):
    data = bytearray(data)
    for offset, value in edits:
        data[offset] = value
    return bytes(data)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("report")
    ap.add_argument(
        "--fixtures",
        default=os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."),
    )
    ap.add_argument("--negatives", type=int, default=4, help="not-a-cause bytes checked per case")
    args = ap.parse_args()
    build_images(args.fixtures)
    rows = json.load(open(args.report))
    failures = 0
    summary = []
    for row in rows:
        ev = row["evaluation"]
        rep = ev["report"]
        name, fixture = ev["name"], ev["fixture"]
        phi = PHI[name]
        original = bytes.fromhex(input_hex(ev, rep))
        checks = []
        if ev["truth"]["refuse"]:
            # Repeat the recorded input natively: a refused case should show
            # both outcomes over time.
            seen = set()
            for _ in range(12):
                out, rc = run_native(fixture, original)
                seen.add(phi(out, rc))
                time.sleep(0.35)
            checks.append(("native outcomes over 12 runs", sorted(seen), len(seen) == 2))
        else:
            out, rc = run_native(fixture, original)
            want = rep["original"]["kind"] == "holds"
            checks.append(("recorded input", phi(out, rc), phi(out, rc) == want))
            for c in rep["candidates"]:
                v = c["verdict"]
                if c["input_offset"] is None or c["location"]["kind"] != "memory":
                    continue
                if v["kind"] == "counterfactual_witness":
                    w = v["witness"]
                    data = apply_inputs(original, [(c["input_offset"], w["intervened_bytes"][0])])
                    out, rc = run_native(fixture, data)
                    checks.append((f"witness in[{c['input_offset']}]", phi(out, rc), phi(out, rc) != want))
            negatives = [
                c for c in rep["candidates"]
                if c["verdict"]["kind"] == "not_a_cause" and c["origin"] == "input_byte"
            ][: args.negatives]
            for c in negatives:
                for value in c["verdict"]["tried"]:
                    data = apply_inputs(original, [(c["input_offset"], value[0])])
                    out, rc = run_native(fixture, data)
                    checks.append((f"not-a-cause in[{c['input_offset']}]={value[0]:#x}", phi(out, rc), phi(out, rc) == want))
            joint = rep["joint"]
            if joint["kind"] == "joint_witness":
                edits = [
                    (off, iv["value"][0])
                    for off, iv in zip(joint["input_offsets"], joint["interventions"])
                ]
                data = apply_inputs(original, edits)
                out, rc = run_native(fixture, data)
                checks.append((f"joint {edits}", phi(out, rc), phi(out, rc) != want))
                for off, val in edits:
                    out, rc = run_native(fixture, apply_inputs(original, [(off, val)]))
                    checks.append((f"joint member alone in[{off}]", phi(out, rc), phi(out, rc) == want))
        ok = all(c[2] for c in checks)
        failures += 0 if ok else 1
        summary.append({"case": name, "fixture": fixture, "checks": len(checks), "ok": ok,
                        "failed": [c for c in checks if not c[2]]})
        print(f"{name:24} {fixture:26} {len(checks):3} checks  {'OK' if ok else 'FAIL'}")
        for c in checks:
            if not c[2]:
                print("    FAIL", c)
    json.dump(summary, sys.stderr, indent=1)
    return 1 if failures else 0


def input_hex(ev, rep):
    return ev["input_hex"]


if __name__ == "__main__":
    sys.exit(main())
