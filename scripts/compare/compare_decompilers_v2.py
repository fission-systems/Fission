#!/usr/bin/env python3
import argparse
import datetime
import difflib
import json
import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path


def detect_python() -> str:
    script_dir = Path(__file__).resolve().parent
    scripts_dir = script_dir.parent
    project_root = scripts_dir.parent
    venv_python = project_root / ".venv" / "bin" / "python"
    if venv_python.exists():
        return str(venv_python)
    return sys.executable


def detect_fission_cmd(project_root: Path) -> list[str]:
    debug_bin = project_root / "target" / "debug" / "fission_cli"
    release_bin = project_root / "target" / "release" / "fission_cli"
    if debug_bin.exists():
        return [str(debug_bin)]
    if release_bin.exists():
        return [str(release_bin)]
    return ["cargo", "run", "--quiet", "--bin", "fission_cli", "--"]


def build_env(project_root: Path) -> dict:
    env = os.environ.copy()
    libdecomp_dir = project_root / "ghidra_decompiler" / "build"
    dyld = env.get("DYLD_LIBRARY_PATH", "")
    if dyld:
        env["DYLD_LIBRARY_PATH"] = f"{libdecomp_dir}:{dyld}"
    else:
        env["DYLD_LIBRARY_PATH"] = str(libdecomp_dir)
    return env


def run_command(cmd: list[str], cwd: Path, env: dict, timeout: int) -> tuple[str, float]:
    start = time.perf_counter()
    try:
        completed = subprocess.run(
            cmd,
            cwd=str(cwd),
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            text=True,
            check=False,
        )
        elapsed = time.perf_counter() - start
        return completed.stdout or "", elapsed
    except subprocess.TimeoutExpired:
        elapsed = time.perf_counter() - start
        return f"ERROR: command timed out after {timeout}s", elapsed


def strip_ansi(text: str) -> str:
    ansi_escape = re.compile(r"\x1b\[[0-9;]*m")
    return ansi_escape.sub("", text).strip()


def strip_inferred_structs(text: str) -> str:
    marker = "// Inferred Structure Definitions"
    if marker not in text:
        return text
    lines = text.splitlines()
    out: list[str] = []
    i = 0
    while i < len(lines):
        line = lines[i]
        if line.strip() == marker:
            i += 1
            while i < len(lines):
                if lines[i].strip() == "":
                    i += 1
                    continue
                if lines[i].startswith("typedef struct"):
                    i += 1
                    while i < len(lines):
                        stripped = lines[i].strip()
                        if stripped == "};":
                            break
                        if stripped.startswith("}") and stripped.endswith(";"):
                            break
                        i += 1
                    if i < len(lines):
                        i += 1
                    continue
                break
            continue
        out.append(line)
        i += 1
    return "\n".join(out).strip()


def strip_banner_and_comments(text: str) -> str:
    lines = text.splitlines()
    cleaned: list[str] = []
    for line in lines:
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith("//") or stripped.startswith("/*") or stripped.startswith("*") or stripped.startswith("*/"):
            continue
        if re.match(r"^[=\-]{3,}$", stripped):
            continue
        if re.match(r"^[╔╗╚╝═║]+$", stripped):
            continue
        cleaned.append(line)
    return "\n".join(cleaned).strip()


def extract_ghidra_parts(ghidra_full: str) -> tuple[str, str]:
    asm = ""
    decomp = ""
    if "--- Assembly Listing ---" in ghidra_full and "--- Decompiled Code ---" in ghidra_full:
        parts = ghidra_full.split("--- Assembly Listing ---")
        if len(parts) > 1:
            asm_and_rest = parts[1].split("--- Decompiled Code ---")
            asm = asm_and_rest[0].strip()
            if len(asm_and_rest) > 1:
                decomp = asm_and_rest[1].strip()
    else:
        decomp = ghidra_full.strip()
        asm = "Assembly not available"
    return asm, decomp


def extract_fission_decomp(text: str) -> str:
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip().startswith("//") and "===" in line:
            return "\n".join(lines[i:]).strip()
    return text.strip()


def strip_fission_noise(text: str) -> str:
    filtered: list[str] = []
    skip_prefixes = (
        "Usage:",
        "Information:",
        "Analysis:",
        "Decompilation:",
        "Output:",
        "Examples:",
    )
    skip_emoji_prefixes = ("📊", "🔍", "⚙️", "💾", "📚")
    for line in text.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        if stripped.startswith(("╔", "║", "╚")):
            continue
        if stripped.startswith(skip_prefixes):
            continue
        if stripped.startswith(skip_emoji_prefixes):
            continue
        if stripped.startswith("  -") or stripped.startswith("  fission"):
            continue
        filtered.append(line)
    return "\n".join(filtered).strip()


def analyze_code(code: str) -> dict:
    lines = code.count("\n") + 1 if code else 0
    chars = len(code)
    functions = code.count("(")
    branches = sum(code.count(kw) for kw in ["if", "while", "for", "switch"])
    return {"lines": lines, "chars": chars, "functions": functions, "branches": branches}


def write_text(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")


def summarize_timings(results: list[dict]) -> dict:
    ghidra_vals = []
    fission_vals = []
    faster_counts = {"ghidra": 0, "fission": 0, "tie": 0}

    for item in results:
        timings = item.get("timings", {})
        ghidra_sec = timings.get("ghidra_sec", 0.0)
        fission_sec = timings.get("fission_decomp_sec", 0.0)
        if ghidra_sec:
            ghidra_vals.append(ghidra_sec)
        if fission_sec:
            fission_vals.append(fission_sec)
        if ghidra_sec and fission_sec:
            if ghidra_sec < fission_sec:
                faster_counts["ghidra"] += 1
            elif fission_sec < ghidra_sec:
                faster_counts["fission"] += 1
            else:
                faster_counts["tie"] += 1

    def stats(values: list[float]) -> dict:
        if not values:
            return {"count": 0, "avg": 0.0, "median": 0.0, "min": 0.0, "max": 0.0}
        return {
            "count": len(values),
            "avg": round(statistics.fmean(values), 3),
            "median": round(statistics.median(values), 3),
            "min": round(min(values), 3),
            "max": round(max(values), 3),
        }

    return {
        "ghidra": stats(ghidra_vals),
        "fission": stats(fission_vals),
        "faster_counts": faster_counts,
    }


def compare_single(binary: Path, address: str, output_json: Path, timeout: int) -> dict:
    script_dir = Path(__file__).resolve().parent
    scripts_dir = script_dir.parent
    project_root = scripts_dir.parent

    python_bin = detect_python()
    fission_cmd = detect_fission_cmd(project_root)
    env = build_env(project_root)

    ghidra_cmd = [
        python_bin,
        str(scripts_dir / "ghidra" / "pyghidra_decompile.py"),
        str(binary),
        address,
    ]
    fission_asm_cmd = fission_cmd + [str(binary), "--disasm-function", address]
    fission_decomp_cmd = fission_cmd + [str(binary), "--decomp", address]

    ghidra_full, ghidra_sec = run_command(ghidra_cmd, project_root, env, timeout)
    fission_asm_raw, fission_asm_sec = run_command(fission_asm_cmd, project_root, env, timeout)
    fission_decomp_raw, fission_decomp_sec = run_command(fission_decomp_cmd, project_root, env, timeout)

    ghidra_full = strip_ansi(ghidra_full)
    fission_asm = strip_fission_noise(strip_ansi(fission_asm_raw))
    fission_decomp_raw = strip_ansi(fission_decomp_raw)

    ghidra_asm, ghidra_decomp = extract_ghidra_parts(ghidra_full)
    fission_decomp = extract_fission_decomp(fission_decomp_raw)

    ghidra_metrics = analyze_code(ghidra_decomp)
    fission_metrics = analyze_code(fission_decomp)

    fission_decomp_similarity = strip_banner_and_comments(strip_inferred_structs(fission_decomp))
    ghidra_decomp_similarity = strip_banner_and_comments(ghidra_decomp)

    ghidra_lines = ghidra_decomp_similarity.splitlines()
    fission_lines = fission_decomp_similarity.splitlines()
    similarity = 0.0
    if ghidra_lines or fission_lines:
        similarity = difflib.SequenceMatcher(None, ghidra_lines, fission_lines).ratio()

    result = {
        "comparison_info": {
            "binary": str(binary),
            "address": address,
            "timestamp": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
            "metrics": {
                "ghidra": ghidra_metrics,
                "fission": fission_metrics,
            },
            "similarity": round(similarity * 100, 2),
        },
        "timings": {
            "ghidra_sec": round(ghidra_sec, 3),
            "fission_asm_sec": round(fission_asm_sec, 3),
            "fission_decomp_sec": round(fission_decomp_sec, 3),
        },
        "ghidra_assembly": ghidra_asm,
        "ghidra_decompilation": ghidra_decomp,
        "fission_assembly": fission_asm,
        "fission_decompilation": fission_decomp,
    }

    output_json.parent.mkdir(parents=True, exist_ok=True)
    with output_json.open("w", encoding="utf-8") as f:
        json.dump(result, f, indent=2, ensure_ascii=False)

    base_path = output_json.with_suffix("")
    write_text(base_path.with_name(base_path.name + "_ghidra_asm.txt"), ghidra_asm)
    write_text(base_path.with_name(base_path.name + "_ghidra_decomp.txt"), ghidra_decomp)
    write_text(base_path.with_name(base_path.name + "_fission_asm.txt"), fission_asm)
    write_text(base_path.with_name(base_path.name + "_fission_decomp.txt"), fission_decomp)

    timestamp = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
    log_lines = [
        f"timestamp: {timestamp}",
        "",
        "command: " + " ".join(ghidra_cmd),
        "---- ghidra output ----",
        ghidra_full,
        "",
        "command: " + " ".join(fission_asm_cmd),
        "---- fission asm output ----",
        strip_ansi(fission_asm_raw),
        "",
        "command: " + " ".join(fission_decomp_cmd),
        "---- fission decomp output ----",
        fission_decomp_raw,
        "",
        f"timing: ghidra={ghidra_sec:.3f}s fission_asm={fission_asm_sec:.3f}s fission_decomp={fission_decomp_sec:.3f}s",
    ]
    write_text(base_path.with_name(base_path.name + "_run.log"), "\n".join(log_lines))

    return result


def parse_address_file(path: Path) -> list[tuple[str, str | None]]:
    items: list[tuple[str, str | None]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        parts = stripped.split()
        addr = parts[0]
        name = parts[1] if len(parts) > 1 else None
        items.append((addr, name))
    return items


def generate_html_report(results: list[dict], output_dir: Path, summary: dict | None = None) -> None:
    rows = []
    for item in results:
        info = item.get("comparison_info", {})
        addr = info.get("address", "unknown")
        sim = info.get("similarity", 0)
        timings = item.get("timings", {})
        ghidra_sec = timings.get("ghidra_sec", 0.0)
        fission_sec = timings.get("fission_decomp_sec", 0.0)
        faster = "tie"
        if ghidra_sec and fission_sec:
            if ghidra_sec < fission_sec:
                faster = "ghidra"
            elif fission_sec < ghidra_sec:
                faster = "fission"
        base = output_dir / f"addr_{addr}.json"
        stem = base.with_suffix("")
        rows.append(
            f"<tr><td>{addr}</td><td>{sim:.2f}%</td><td>{ghidra_sec:.3f}s</td>"
            f"<td>{fission_sec:.3f}s</td><td>{faster}</td>"
            f"<td><a href='{stem.name}_ghidra_decomp.txt'>Ghidra</a></td>"
            f"<td><a href='{stem.name}_fission_decomp.txt'>Fission</a></td></tr>"
        )
    html = [
        "<!doctype html>",
        "<html><head><meta charset='utf-8'>",
        "<title>Decompiler Comparison Report</title>",
        "<style>body{font-family:Arial,sans-serif;padding:20px}table{border-collapse:collapse;width:100%}"
        "th,td{border:1px solid #ddd;padding:8px}th{background:#f5f5f5}</style>",
        "</head><body>",
        "<h1>Decompiler Comparison Report</h1>",
    ]
    if summary:
        gh = summary.get("ghidra", {})
        fi = summary.get("fission", {})
        faster = summary.get("faster_counts", {})
        html.extend(
            [
                "<h2>Timing Summary</h2>",
                "<ul>",
                f"<li>Ghidra avg: {gh.get('avg', 0.0):.3f}s (median {gh.get('median', 0.0):.3f}s)</li>",
                f"<li>Fission avg: {fi.get('avg', 0.0):.3f}s (median {fi.get('median', 0.0):.3f}s)</li>",
                f"<li>Faster: ghidra {faster.get('ghidra', 0)}, "
                f"fission {faster.get('fission', 0)}, tie {faster.get('tie', 0)}</li>",
                "</ul>",
            ]
        )
    html.extend(
        [
            "<table><thead><tr><th>Address</th><th>Similarity</th><th>Ghidra(s)</th>"
            "<th>Fission(s)</th><th>Faster</th><th>Ghidra</th><th>Fission</th></tr></thead><tbody>",
            *rows,
            "</tbody></table></body></html>",
        ]
    )
    report_path = output_dir / "report.html"
    report_path.write_text("\n".join(html), encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Compare Ghidra vs Fission Decompiler (Python integration)",
        add_help=False,
    )
    parser.add_argument("--help", action="help", help="Show this help message and exit")
    parser.add_argument("binary", nargs="?", help="Binary path")
    parser.add_argument("address_or_file", nargs="?", help="Address or address file")
    parser.add_argument("output", nargs="?", help="Output JSON or output directory")
    parser.add_argument("-m", "--batch", action="store_true", help="Batch mode")
    parser.add_argument("-h", "--html", action="store_true", help="Generate HTML report (batch mode)")
    parser.add_argument("-t", "--timeout", type=int, default=600, help="Timeout seconds")
    args = parser.parse_args()

    if not args.binary or not args.address_or_file:
        parser.print_help()
        return 1

    binary = Path(args.binary).expanduser().resolve()
    if not binary.exists():
        print(f"Error: Binary file not found: {binary}", file=sys.stderr)
        return 1

    if args.batch:
        scripts_dir = Path(__file__).resolve().parent.parent
        output_dir = Path(args.output or (scripts_dir / "result")).expanduser()
        output_dir.mkdir(parents=True, exist_ok=True)
        addr_file = Path(args.address_or_file).expanduser()
        entries = parse_address_file(addr_file)
        results: list[dict] = []
        for addr, _name in entries:
            output_json = output_dir / f"addr_{addr}.json"
            print(f"== {addr} ==")
            results.append(compare_single(binary, addr, output_json, args.timeout))
        summary = summarize_timings(results)
        summary_path = output_dir / "summary.json"
        summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False), encoding="utf-8")
        print("")
        print("Timing summary (batch):")
        gh = summary.get("ghidra", {})
        fi = summary.get("fission", {})
        faster = summary.get("faster_counts", {})
        print(
            "  Ghidra: avg {avg:.3f}s, median {median:.3f}s, min {min:.3f}s, max {max:.3f}s".format(
                **gh
            )
        )
        print(
            "  Fission: avg {avg:.3f}s, median {median:.3f}s, min {min:.3f}s, max {max:.3f}s".format(
                **fi
            )
        )
        print(
            f"  Faster: ghidra {faster.get('ghidra', 0)}, "
            f"fission {faster.get('fission', 0)}, tie {faster.get('tie', 0)}"
        )
        if args.html:
            generate_html_report(results, output_dir, summary)
        return 0

    output_json = Path(args.output) if args.output else None
    if output_json is None:
        timestamp = datetime.datetime.now().strftime("%Y%m%d%H%M")
        scripts_dir = Path(__file__).resolve().parent.parent
        result_dir = scripts_dir / "result" / f"{timestamp}_result"
        result_dir.mkdir(parents=True, exist_ok=True)
        output_json = result_dir / "comparison.json"

    result = compare_single(binary, args.address_or_file, output_json, args.timeout)
    info = result.get("comparison_info", {})
    metrics = info.get("metrics", {})
    similarity = info.get("similarity", 0.0)
    timings = result.get("timings", {})
    ghidra_sec = timings.get("ghidra_sec", 0.0)
    fission_sec = timings.get("fission_decomp_sec", 0.0)
    faster = "tie"
    if ghidra_sec and fission_sec:
        if ghidra_sec < fission_sec:
            faster = "ghidra"
        elif fission_sec < ghidra_sec:
            faster = "fission"

    print("")
    print("==========================================")
    print("✅ Comparison Complete")
    print("==========================================")
    print(f"  JSON: {output_json}")
    print("  Text Extracts:")
    base = output_json.with_suffix("")
    print(f"    • {base}_ghidra_asm.txt")
    print(f"    • {base}_ghidra_decomp.txt")
    print(f"    • {base}_fission_asm.txt")
    print(f"    • {base}_fission_decomp.txt")
    if metrics:
        g = metrics.get("ghidra", {})
        f = metrics.get("fission", {})
        print("")
        print("Metrics:")
        print(f"  Ghidra:  {g.get('lines', 0)} lines, {g.get('branches', 0)} branches")
        print(f"  Fission: {f.get('lines', 0)} lines, {f.get('branches', 0)} branches")
        print(f"  Similarity: {similarity:.2f}%")
    if ghidra_sec or fission_sec:
        print("")
        print("Timing:")
        print(f"  Ghidra decomp:  {ghidra_sec:.3f}s")
        print(f"  Fission decomp: {fission_sec:.3f}s")
        print(f"  Faster: {faster}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
