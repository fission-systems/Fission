#!/usr/bin/env python3
"""Manual Metal experiment. Never imported by Fission's loader or CLI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import shutil
import sys
import statistics
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--signatures", type=Path, required=True)
    parser.add_argument("--sample", type=Path, required=True)
    parser.add_argument("--corpus-root", type=Path, required=True)
    parser.add_argument("--timing-cells", default="41,26,22")
    parser.add_argument("--reverse-first", action="store_true")
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--warm-iterations", type=int, default=7)
    args = parser.parse_args()
    if args.rounds < 1 or args.warm_iterations < 2:
        parser.error("rounds >=1 and warm-iterations >=2 required")
    root = Path(__file__).resolve().parents[4]
    source = Path(__file__).resolve().parent
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    (out / "bin").mkdir()
    commands = []
    if sys.platform != "darwin":
        parser.error("the standalone Metal prototype requires macOS")
    source_files = [source / "run.py", source / "LiteralScan.metal", source / "MetalLiteralScan.swift",
                    root / "crates/fission-loader/examples/literal_scan_cpu.rs"]
    source_hashes = {str(p.relative_to(root)): sha(p) for p in source_files}
    for p in source_files:
        saved = out / "source" / p.relative_to(root)
        saved.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(p, saved)
    (out / "source-sha256.json").write_text(json.dumps(source_hashes, indent=2) + "\n")

    def build(command, name):
        commands.append(command)
        with (out / name).open("wb") as log:
            subprocess.run(command, cwd=root, stdout=log, stderr=subprocess.STDOUT, check=True)

    build(["cargo", "build", "--locked", "--release", "-p", "fission-loader",
           "--example", "literal_scan_cpu"], "build-cpu.log")
    cpu = out / "bin/literal_scan_cpu"
    shutil.copy2(root / "target/release/examples/literal_scan_cpu", cpu)
    gpu = out / "bin/metal-literal-scan"
    build(["xcrun", "swiftc", "-O", "-whole-module-optimization", "-framework", "Metal",
           "-framework", "Foundation", str(source / "MetalLiteralScan.swift"), "-o", str(gpu)], "build-metal.log")
    shader = out / "bin/LiteralScan.metal"
    shutil.copy2(source / "LiteralScan.metal", shader)
    signatures = json.loads(args.signatures.read_text())
    patterns = sorted({rule["value"] for sig in signatures["signatures"] for rule in sig["rules"]
                       if rule["type"] == "string" and rule["value"]})
    patterns_file = out / "patterns.json"
    patterns_file.write_text(json.dumps(patterns, ensure_ascii=True) + "\n")
    rows = json.loads(args.sample.read_text())["cells"]
    fixtures = out / "fixtures"
    fixtures.mkdir()
    outputs = out / "runs"
    outputs.mkdir()
    runs = []

    def run(backend, pattern_path, input_path, iterations, tag):
        if backend.startswith("cpu"):
            command = [str(cpu), str(pattern_path), str(input_path), str(iterations),
                       "14" if backend == "cpu14" else "1"]
        else:
            command = [str(gpu), str(pattern_path), str(input_path), str(iterations),
                       "resident" if backend == "gpu-resident" else "upload", str(shader)]
        start = time.perf_counter()
        proc = subprocess.run(command, capture_output=True, timeout=120)
        wall = (time.perf_counter() - start) * 1000
        (outputs / (tag + ".json")).write_bytes(proc.stdout)
        (outputs / (tag + ".stderr")).write_bytes(proc.stderr)
        if proc.returncode:
            raise RuntimeError(f"{tag}: exit {proc.returncode}: {proc.stderr.decode(errors='replace')}")
        value = json.loads(proc.stdout)
        runs.append({"tag": tag, "backend": backend, "process_wall_ms": wall, "command": command})
        return value, wall

    cases = [
        (b"", []), (b"", ["x", "long"]), (b"x", ["x", "xy", "z"]),
        (b"aaaaa", ["a", "aa", "aaaaa", "aaaaaa", "a"]),
        (b"\xff\x00A\xfe" + "é漢字".encode(), ["\u0000A", "é", "漢字", "É", ".*"]),
        (b"a.*b[0]$^\\", ["a.*b", "[0]", "$^", "\\", "a+b", "a.b"]),
        (b"x" * 4096, ["x" * 4097, "x" * 4096, "y"]),
        (b"\0" * 8190 + b"end", ["\u0000", "\u0000\u0000", "end", "ends"]),
    ]
    boundary = bytearray(b"\x7f" * 4099)
    for offset, literal in [(253, b"gpu-boundary"), (291, b"cpu-boundary"), (4088, b"last-offset")]:
        boundary[offset:offset + len(literal)] = literal
    cases.append((bytes(boundary), ["gpu-boundary", "cpu-boundary", "last-offset", "absent"]))
    rng = random.Random(0x4D4554414C)
    for _ in range(12):
        data = bytearray(rng.randbytes(rng.randrange(1, 4097)))
        literal = "probe-" + str(rng.randrange(100000))
        pos = rng.randrange(len(data) + 1)
        data[pos:pos] = literal.encode()
        cases.append((bytes(data), [literal, "not-in-input", literal[:3], "\u0000", "é"]))
    backends = ["cpu1", "cpu14", "gpu-upload", "gpu-resident"]
    for i, (data, needles) in enumerate(cases):
        path = fixtures / f"{i}.bin"
        path.write_bytes(data)
        pp = fixtures / f"{i}.json"
        pp.write_text(json.dumps(needles) + "\n")
        expected = [p.encode() in data for p in needles]
        for backend in backends:
            value, _ = run(backend, pp, path, 2, f"fixture-{i}-{backend}")
            assert all(s["found"] == expected for s in value["scans"]), (i, backend)
        print(f"fixture {i} exact", flush=True)

    manifest = []
    references = {}
    for cell in rows:
        path = (args.corpus_root / cell["input"]).resolve()
        cpu_value, _ = run("cpu1", patterns_file, path, 1, f"parity-{cell['id']}-cpu1")
        gpu_value, _ = run("gpu-upload", patterns_file, path, 1, f"parity-{cell['id']}-gpu")
        expected = cpu_value["scans"][0]["found"]
        assert expected == gpu_value["scans"][0]["found"], cell["id"]
        references[cell["id"]] = expected
        manifest.append({"cell": cell["id"], "input": cell["input"], "bytes": path.stat().st_size,
                         "sha256": sha(path), "found": expected})
        print(f"corpus {cell['id']} exact", flush=True)
    measurements = []
    selected = args.timing_cells.split(",")
    assert set(selected) <= {c["id"] for c in rows}
    for cell in (c for c in rows if c["id"] in selected):
        path = (args.corpus_root / cell["input"]).resolve()
        for mode, iterations in [("fresh", 1), ("warm", args.warm_iterations)]:
            for i in range(args.rounds):
                order = backends if (i + args.reverse_first) % 2 == 0 else list(reversed(backends))
                for backend in order:
                    tag = f"timing-{cell['id']}-{mode}-{i}-{backend}"
                    value, wall = run(backend, patterns_file, path, iterations, tag)
                    assert all(s["found"] == references[cell["id"]] for s in value["scans"]), tag
                    operations = [s.get("operation_ms", s["scan_ms"]) for s in value["scans"]]
                    measurements.append({"cell": cell["id"], "mode": mode, "round": i,
                        "backend": backend, "tag": tag, "process_wall_ms": wall,
                        "read_ms": value["read_ms"], "setup_ms": value["setup_ms"],
                        "operation_ms": statistics.median(operations[1:] if mode == "warm" else operations),
                        "scans": value["scans"]})
                    print(tag, round(wall, 3), flush=True)
    assert source_hashes == {str(p.relative_to(root)): sha(p) for p in source_files}, "source changed during run"
    summary = {
        "scope": "Exact literal-presence primitive only; production loader/CLI unchanged.",
        "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "compiler": subprocess.check_output(["rustc", "-Vv"], text=True),
        "swift": subprocess.check_output(["xcrun", "swiftc", "--version"], text=True),
        "source_sha256": source_hashes,
        "reverse_first": args.reverse_first,
        "binary_sha256": {"cpu": sha(cpu), "gpu": sha(gpu), "shader": sha(shader)},
        "sample_sha256": sha(args.sample), "signature_sha256": sha(args.signatures),
        "patterns": patterns, "fixture_cases": len(cases), "fixture_scans": len(cases) * 8,
        "all_parity_equal": True, "corpus": manifest, "measurements": measurements,
        "runs": runs, "build_commands": commands,
        "bounds": ["No full loader, identity policy, decompiler output, score or execution claim.",
            "CPU comparator uses existing RegexSet literal operator; cpu14 is an experimental parallel variant.",
            "GPU upload includes allocation/copy/submit/wait/result read; resident reuses input buffer.",
            "Fresh process includes dynamic linking, input/pattern read, runtime Metal library/PSO setup and JSON output.",
            "GPU driver/shader and OS caches are uncontrolled; fresh process is not guaranteed cold hardware.",
            "Warm excludes first operation and reuses matcher/pipeline; resident-input timing is a separate scenario.",
            "No own build or profiler overlaps timing. File IO differs by runtime and is reported separately.",
            "UInt32 single-dispatch/input buffer limits; no wildcard, full regex or full DIE-rule support."]}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print("all literal presence checks exact; raw report:", out / "summary.json", flush=True)


if __name__ == "__main__":
    main()
