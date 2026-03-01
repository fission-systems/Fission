#!/usr/bin/env python3
"""
benchmark_v4.py — Fission vs Ghidra decompiler quality benchmark (v4).

Key differences from v3:
  - Ghidra results read from pre-cached JSON (no live PyGhidra execution)
  - No speed measurement focus — timing is recorded but not a primary metric
  - Supports all formats: Mach-O ARM64/x64, ELF x64, PE x64
  - Category-level aggregation (arithmetic, control_flow, structs, etc.)
  - Auto-generated suite YAML with symbol extraction

Usage:
    # Run benchmark from a suite YAML
    python3 benchmark_v4.py --suite suites/suite_macos_arm64.yaml \\
        --cache benchmark_cache/ -o results/

    # Run for a specific category only
    python3 benchmark_v4.py --suite suites/suite_all.yaml \\
        --cache benchmark_cache/ --category control

    # Generate HTML report
    python3 benchmark_v4.py --suite suites/suite_all.yaml \\
        --cache benchmark_cache/ -o results/ --html
"""

from __future__ import annotations

import argparse
import datetime
import difflib
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

try:
    import yaml as _yaml
    _YAML_AVAILABLE = True
except ImportError:
    _YAML_AVAILABLE = False


# ===========================================================================
# Environment / path helpers
# ===========================================================================

def _project_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


def detect_fission_cmd() -> list[str]:
    root = _project_root()
    for rel in ("target/release/fission_cli", "target/debug/fission_cli"):
        p = root / rel
        if p.exists():
            return [str(p)]
    return ["cargo", "run", "--quiet", "--bin", "fission_cli", "--"]


def build_env() -> dict[str, str]:
    env = os.environ.copy()
    libdecomp_dir = str(_project_root() / "ghidra_decompiler" / "build")
    for var in ("DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"):
        existing = env.get(var, "")
        env[var] = f"{libdecomp_dir}:{existing}" if existing else libdecomp_dir
    return env


def env_info() -> dict[str, str]:
    root = _project_root()
    fission_rev = "unknown"
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=str(root), capture_output=True, text=True,
        )
        if result.returncode == 0:
            fission_rev = result.stdout.strip()
    except OSError:
        pass

    return {
        "python": platform.python_version(),
        "platform": platform.platform(),
        "fission_git_rev": fission_rev,
        "run_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
    }


def load_suite(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    if path.suffix in (".yaml", ".yml") and _YAML_AVAILABLE:
        return _yaml.safe_load(text)
    return json.loads(text)


# ===========================================================================
# Fission CLI execution
# ===========================================================================

def run_fission(binary: str, address: str, timeout: int = 60) -> tuple[str, float]:
    """Run Fission decompiler and return (output, elapsed_sec)."""
    cmd = detect_fission_cmd() + [binary, "--decomp", address, "--no-header"]
    start = time.perf_counter()
    try:
        result = subprocess.run(
            cmd, cwd=str(_project_root()), env=build_env(),
            capture_output=True, text=True,
            timeout=timeout, check=False,
        )
        elapsed = time.perf_counter() - start
        output = result.stdout or ""
        if result.returncode != 0 and not output.strip():
            output = f"ERROR: fission_cli exited with code {result.returncode}\n{result.stderr or ''}"
        return output, elapsed
    except subprocess.TimeoutExpired:
        return f"ERROR: timeout after {timeout}s", time.perf_counter() - start
    except FileNotFoundError:
        return "ERROR: fission_cli not found", 0.0


# ===========================================================================
# Ghidra cache reader
# ===========================================================================

def load_ghidra_cache(cache_dir: Path, address: str) -> dict[str, Any] | None:
    """Load cached Ghidra decompilation result."""
    norm_addr = address.lower()
    cache_file = cache_dir / f"ghidra_{norm_addr}.json"
    if cache_file.exists():
        with open(cache_file, "r", encoding="utf-8") as f:
            return json.load(f)
    return None


# ===========================================================================
# Text normalization (ported from v3)
# ===========================================================================

def strip_ansi(text: str) -> str:
    return re.compile(r"\x1b\[[0-9;]*m").sub("", text).strip()


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
                    i += 1; continue
                if lines[i].startswith("typedef struct"):
                    i += 1
                    while i < len(lines) and not lines[i].strip().startswith("}"):
                        i += 1
                    if i < len(lines): i += 1
                    continue
                break
            continue
        out.append(line)
        i += 1
    return "\n".join(out).strip()


def strip_banner_and_comments(text: str) -> str:
    cleaned: list[str] = []
    for line in text.splitlines():
        s = line.strip()
        if not s:
            continue
        if s.startswith(("//", "/*")):
            continue
        if s.startswith("*/"):
            continue
        if s.startswith("* ") or s == "*":
            continue
        if re.match(r"^[=\-]{3,}$", s) or re.match(r"^[╔╗╚╝═║]+$", s):
            continue
        cleaned.append(line)
    return "\n".join(cleaned).strip()


def strip_fission_noise(text: str) -> str:
    filtered: list[str] = []
    skip_pfx = ("Usage:", "Information:", "Analysis:", "Decompilation:", "Output:", "Examples:")
    skip_emoji = ("📊", "🔍", "⚙️", "💾", "📚")
    for line in text.splitlines():
        s = line.strip()
        if not s:
            continue
        if s.startswith(("╔", "║", "╚")):
            continue
        if s.startswith(skip_pfx) or s.startswith(skip_emoji):
            continue
        if s.startswith(("  -", "  fission")):
            continue
        filtered.append(line)
    return "\n".join(filtered).strip()


def extract_fission_decomp(text: str) -> str:
    result = []
    for line in text.splitlines():
        s = line.strip()
        if s.startswith("//") and ("===" in s or (s.startswith("// Function:") and "@" in s)):
            continue
        result.append(line)
    return "\n".join(result).strip()


_C_KEYWORDS = {
    "if", "for", "while", "do", "switch", "return", "sizeof", "typeof",
    "else", "case", "break", "continue", "goto", "typedef", "struct",
    "union", "enum", "extern", "static", "inline", "void", "int", "char",
    "float", "double", "long", "short", "unsigned", "signed", "const",
    "auto", "register", "volatile", "restrict", "VAR", "FUNC", "UNDEF",
    "FUNCNAME", "OPAQUE_PTR",
}


def normalize_for_similarity(text: str) -> str:
    """Normalize decompiler output for fair comparison (from v3)."""
    text = strip_inferred_structs(text)
    text = strip_banner_and_comments(text)
    lines = [re.sub(r"\s+", " ", l.strip()) for l in text.splitlines() if l.strip()]
    text = "\n".join(lines)
    text = re.sub(r",\s+", ",", text)

    # Variable name normalization
    for pat in (r"\blocal_[0-9a-f]+\b", r"\buVar[0-9]+\b", r"\biVar[0-9]+\b",
                r"\bpVar[0-9]+\b", r"\bdVar[0-9]+\b"):
        text = re.sub(pat, "VAR", text)
    text = re.sub(r"\bparam_[0-9]+\b", "VAR", text)
    text = re.sub(r"\barg_[0-9]+\b", "VAR", text)
    text = re.sub(r"\bin_[A-Z][A-Za-z0-9]+\b", "VAR", text)

    # Calling convention annotations
    text = re.sub(r"\b__(?:cdecl|fastcall|stdcall|thiscall)\b\s*", "", text)

    # Function names
    text = re.sub(r"\bsub_[0-9a-fA-F]+\b", "FUNC", text)
    text = re.sub(r"\(\*[A-Za-z0-9_.\-]+\.[Dd][Ll][Ll]![A-Za-z_]\w*\)\s*(?=\()", "FUNC", text)
    text = re.sub(r"\b_([a-zA-Z]\w*)\b", r"\1", text)

    def _norm_callable(m: re.Match) -> str:
        name = m.group(1)
        return m.group(0) if name in _C_KEYWORDS else "FUNC("
    text = re.sub(r"\b([A-Za-z_]\w*)\s*\(", _norm_callable, text)

    # Fission rename-pass variables
    for pat in (r"\bresult\b", r"\bretval\b"):
        text = re.sub(pat, "VAR", text)
    text = re.sub(r"\bVAR\+\+", "VAR = VAR + 1", text)

    # Type normalization
    text = re.sub(r"\b(?:void|undefined\d*)\s*\*\s*", "OPAQUE_PTR ", text)
    text = re.sub(r"\bp[vucslt]Var[0-9]*\b", "VAR", text)
    text = re.sub(r"\b[a-z][A-Za-z]?Var[0-9]*\b", "VAR", text)

    int_cast_pat = r"\((?:longlong|ulonglong|uint|ushort|uchar|sbyte|longdouble)\)\s*"
    text = re.sub(int_cast_pat, "", text)
    text = re.sub(r"\bundefined[0-9]+\b(?!\s*\*)", "UNDEF", text)
    text = re.sub(r"\b(?:longlong|ulonglong)\b(?!\s*\*)", "UNDEF", text)
    text = re.sub(r"\b(?:char|byte|uchar|CHAR)\s*\*\s*", "OPAQUE_PTR ", text)
    text = re.sub(r"\bf_[0-9a-f]+\w*\s*\*\s*", "OPAQUE_PTR ", text)
    text = re.sub(r"\bUNDEF\s*\*\s*", "OPAQUE_PTR ", text)
    text = re.sub(r"\bOPAQUE_PTR\b", "UNDEF", text)
    text = re.sub(r"\b(?:uint|ushort|ulong|uchar)\s*\*\s*", "UNDEF ", text)
    text = re.sub(r"\(UNDEF\s*\)\s*", "", text)

    # Null pointer comparison
    text = re.sub(r"\s*!=\s*\([^()]+\)\s*0[xX]0\b", "", text)
    text = re.sub(r"\s*!=\s*0[xX]0\b", "", text)
    text = re.sub(r"\(\s+", "(", text)
    text = re.sub(r"\s+\)", ")", text)
    text = re.sub(r"  +", " ", text)
    text = re.sub(r"\b[a-z]\b", "VAR", text)

    # Float/int types
    text = re.sub(r"\bdouble\b(?!\s*\*)", "UNDEF", text)
    text = re.sub(r"\bfloat\b(?!\s*\*)", "UNDEF", text)
    text = re.sub(r"\bint\b(?!\s*\*)", "UNDEF", text)

    # Struct types
    text = re.sub(
        r"\b(?!(?:VAR|FUNC|UNDEF|OPAQUE_PTR)\b)([A-Z][a-zA-Z0-9_]*)\s*\*\s*",
        "UNDEF ", text,
    )
    text = re.sub(r"\bUNDEF\s*\*\s*", "UNDEF ", text)
    text = re.sub(r"\(UNDEF\s*\)\s*", "", text)

    # Debug symbol param names
    _SIG_TYPE_WORDS = {
        "void", "int", "char", "float", "double", "long", "short", "unsigned",
        "signed", "const", "volatile", "struct", "union", "enum", "typedef",
        "static", "extern", "inline", "restrict",
        "VAR", "FUNC", "UNDEF", "OPAQUE_PTR",
        "if", "else", "for", "while", "do", "switch", "case", "break",
        "continue", "return", "goto",
    }
    _names_to_normalize: set[str] = set()
    _brace = text.find("{")
    if _brace > 0:
        _sig = text[:_brace]
        _p1 = _sig.find("(")
        _p2 = _sig.rfind(")")
        if 0 <= _p1 < _p2:
            _params_raw = _sig[_p1 + 1:_p2]
            for _param in _params_raw.split(","):
                _idents = re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", _param)
                if _idents:
                    _pname = _idents[-1]
                    if _pname not in _SIG_TYPE_WORDS:
                        _names_to_normalize.add(_pname)
    for _decl_line in text.splitlines():
        _dm = re.match(r"^\s*UNDEF\s+([A-Za-z_][A-Za-z0-9_]*)\s*;\s*$", _decl_line)
        if _dm:
            _lname = _dm.group(1)
            if _lname not in _SIG_TYPE_WORDS:
                _names_to_normalize.add(_lname)
    for _vname in _names_to_normalize:
        text = re.sub(r"\b" + re.escape(_vname) + r"\b", "VAR", text)

    # Struct field access normalization
    text = re.sub(r"\bVAR->([A-Za-z_]\w*)", "VAR->VAR", text)
    text = re.sub(r"\*\(VAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\)", "VAR->VAR", text)
    text = re.sub(r"\*\(VAR \+ [1-9][0-9]*\)", "VAR->VAR", text)
    text = re.sub(r"\(VAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\)", "VAR->VAR", text)
    text = re.sub(r"\(VAR \+ [1-9][0-9]*\)", "VAR->VAR", text)
    text = re.sub(r"\bVAR \+ 0[xX][1-9a-fA-F][0-9a-fA-F]*\b", "VAR->VAR", text)
    text = re.sub(r"\bVAR \+ [1-9][0-9]*\b", "VAR->VAR", text)
    text = re.sub(r"(?<![&*])\*VAR(?!\s*[->\[])", "VAR->VAR", text)

    # Inline temp-copy
    _lines = text.splitlines()
    _filtered: list[str] = []
    _i = 0
    while _i < len(_lines):
        _ln = _lines[_i]
        if _ln.strip() == "VAR = VAR->VAR;":
            _i += 1
            if _i < len(_lines):
                _next = _lines[_i]
                _next = re.sub(r",\s*VAR\s*\)", ",VAR->VAR)", _next, count=1)
                _filtered.append(_next)
                _i += 1
                continue
            continue
        _filtered.append(_ln)
        _i += 1
    text = "\n".join(_filtered)

    # Clean up
    text = re.sub(r"(?m)^\s*UNDEF VAR;\s*\n?", "", text)
    text = re.sub(r"\n{3,}", "\n\n", text).strip()
    text = re.sub(r"(?m)^\s*VAR = VAR;\s*\n?", "", text)
    text = re.sub(r"\n{3,}", "\n\n", text).strip()

    # Fix over-applied struct access on type declarations
    _c_types = r"(?:int|long|short|char|float|double|size_t|uint|int32_t|int64_t|uint32_t|uint64_t|longlong|ulong|uint|ushort|uchar|word|dword|qword|bool|void)"
    text = re.sub(r"\b(" + _c_types + r"(?:\s*\*)*)\s+VAR->VAR\b", r"\1 VAR", text)

    # Compound assignments
    text = re.sub(r"\bVAR\s*\+=\s*", "VAR = VAR + ", text)
    text = re.sub(r"\bVAR\s*-=\s*", "VAR = VAR - ", text)
    text = re.sub(r"\bVAR\s*\*=\s*", "VAR = VAR * ", text)
    text = re.sub(r"\bVAR\s*&=\s*", "VAR = VAR & ", text)
    text = re.sub(r"\bVAR\s*\|=\s*", "VAR = VAR | ", text)

    # Array null terminator
    text = re.sub(r"VAR->VAR\[[^\]]+\] = '\\0';", "VAR->VAR = 0;", text)
    text = re.sub(r"\bVAR\[[^\]]+\] = '\\0';", "VAR = 0;", text)

    # Return 0 at end
    _lines = text.splitlines()
    if len(_lines) >= 2 and _lines[-1].strip() == "}" and _lines[-2].strip() == "return 0;":
        _lines = _lines[:-2] + [_lines[-1]]
        text = "\n".join(_lines)

    return text


# ===========================================================================
# Quality metrics
# ===========================================================================

def _max_nesting_depth(code: str) -> int:
    depth = max_depth = 0
    for ch in code:
        if ch == "{":
            depth += 1
            max_depth = max(max_depth, depth)
        elif ch == "}" and depth:
            depth -= 1
    return max_depth


def _count_errors(text: str) -> int:
    return len(re.findall(
        r"\b(ERROR|error|Warning|WARNING|WARN|failed|timeout|exception)\b",
        text,
    ))


def analyze_code(code: str, raw_output: str = "") -> dict[str, Any]:
    lines = code.count("\n") + 1 if code else 0
    return {
        "lines": lines,
        "chars": len(code),
        "branches": sum(code.count(kw) for kw in ("if", "while", "for", "switch")),
        "control_flow_depth": _max_nesting_depth(code),
        "goto_count": len(re.findall(r"\bgoto\b", code)),
        "struct_accesses": len(re.findall(r"->|\.\w+\s*[=;(,)\[\]]", code)),
        "casts": len(re.findall(r"\*\s*\(\s*\w+\s*\*\s*\)", code)),
        "string_literals": len(re.findall(r'"[^"]*"', code)),
        "printf_calls": len(re.findall(r"\b(?:printf|fprintf|sprintf|snprintf|puts)\s*\(", code)),
        "tool_errors": _count_errors(raw_output),
    }


def checklist_score(code: str, expected_patterns: list[str]) -> dict[str, Any]:
    hits: dict[str, bool] = {}
    for pat in expected_patterns:
        # Support OR alternatives: "a|b" means either a or b present
        if "|" in pat:
            alts = pat.split("|")
            hits[pat] = any(re.search(re.escape(a), code) for a in alts)
        else:
            hits[pat] = bool(re.search(re.escape(pat), code))
    total = len(expected_patterns)
    satisfied = sum(hits.values())
    return {
        "patterns": hits,
        "satisfied": satisfied,
        "total": total,
        "ratio": round(satisfied / total, 3) if total else 1.0,
    }


# ===========================================================================
# Core comparison
# ===========================================================================

def compare_function(
    binary_path: str,
    func_info: dict,
    cache_dir: Path,
    timeout: int = 60,
) -> dict[str, Any]:
    """Compare Ghidra (cached) vs Fission output for a single function."""
    address = func_info["address"]
    name = func_info.get("name", address)
    expected = func_info.get("expected_patterns", [])

    result: dict[str, Any] = {
        "function": name,
        "address": address,
    }

    # 1. Load Ghidra from cache
    ghidra_data = load_ghidra_cache(cache_dir, address)
    if ghidra_data:
        ghidra_code = ghidra_data.get("code", "")
        ghidra_is_placeholder = ghidra_data.get("placeholder", False)
        result["ghidra_name"] = ghidra_data.get("name", name)
    else:
        ghidra_code = f"// No cached result for {address}"
        ghidra_is_placeholder = True
        result["ghidra_name"] = name

    result["ghidra_placeholder"] = ghidra_is_placeholder

    # 2. Run Fission
    fission_raw, fission_sec = run_fission(binary_path, address, timeout)
    fission_code = extract_fission_decomp(strip_fission_noise(strip_ansi(fission_raw)))

    result["fission_sec"] = round(fission_sec, 4)
    result["fission_code"] = fission_code
    result["ghidra_code"] = ghidra_code

    # 3. Compute similarity (only if Ghidra has real output)
    if not ghidra_is_placeholder and ghidra_code.strip():
        norm_ghidra = normalize_for_similarity(ghidra_code)
        norm_fission = normalize_for_similarity(fission_code)

        if norm_ghidra and norm_fission:
            sim = difflib.SequenceMatcher(None, norm_ghidra, norm_fission).ratio() * 100
        elif not norm_ghidra and not norm_fission:
            sim = 100.0
        else:
            sim = 0.0
        result["similarity"] = round(sim, 2)
    else:
        result["similarity"] = None

    # 4. Quality metrics
    result["fission_metrics"] = analyze_code(fission_code, fission_raw)
    if not ghidra_is_placeholder:
        result["ghidra_metrics"] = analyze_code(ghidra_code)

    # 5. Checklist
    if expected:
        result["fission_checklist"] = checklist_score(fission_code, expected)
        if not ghidra_is_placeholder:
            result["ghidra_checklist"] = checklist_score(ghidra_code, expected)

    # 6. Error detection
    result["fission_has_error"] = bool(re.search(r"\bERROR\b", fission_raw, re.IGNORECASE))

    return result


def run_benchmark(
    suite: dict,
    cache_root: Path,
    output_dir: Path,
    timeout: int = 60,
    category_filter: str | None = None,
) -> dict[str, Any]:
    """Run benchmark for all functions in a suite."""
    root = _project_root()
    results: list[dict] = []
    binary_count = 0
    func_count = 0

    for binary_def in suite.get("binaries", []):
        bin_path = root / binary_def["path"]
        if not bin_path.exists():
            print(f"  SKIP {binary_def['path']}: not found")
            continue

        # Category filter
        category = binary_def.get("category", "unknown")
        if category_filter and category != category_filter:
            continue

        binary_name = bin_path.stem
        cache_dir = cache_root / binary_name
        functions = binary_def.get("functions", [])

        if not functions:
            continue

        binary_count += 1
        print(f"\n  [{binary_name}] ({category}, {binary_def.get('opt_level', '?')}) — {len(functions)} functions")

        for func_info in functions:
            func_count += 1
            fname = func_info.get("name", func_info["address"])
            sys.stdout.write(f"    {fname:30s}")
            sys.stdout.flush()

            result = compare_function(str(bin_path), func_info, cache_dir, timeout)
            result["binary"] = binary_def["path"]
            result["category"] = category
            result["opt_level"] = binary_def.get("opt_level", "unknown")
            result["format"] = binary_def.get("format", "unknown")
            result["arch"] = binary_def.get("arch", "unknown")

            sim_str = f"{result['similarity']:.1f}%" if result.get("similarity") is not None else "N/A"
            err_str = " ERR" if result.get("fission_has_error") else ""
            chk = result.get("fission_checklist", {})
            chk_str = f" chk={chk.get('satisfied', 0)}/{chk.get('total', 0)}" if chk else ""
            print(f"  sim={sim_str}{chk_str}{err_str}")

            results.append(result)

    # Save individual results
    output_dir.mkdir(parents=True, exist_ok=True)
    results_path = output_dir / "results.json"
    with open(results_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)

    # Generate summary
    summary = summarize_results(results)
    summary["env"] = env_info()
    summary["suite"] = suite.get("name", "unnamed")
    summary["binary_count"] = binary_count
    summary["function_count"] = func_count

    summary_path = output_dir / "summary.json"
    with open(summary_path, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)

    return summary


# ===========================================================================
# Summary aggregation
# ===========================================================================

def summarize_results(results: list[dict]) -> dict[str, Any]:
    """Aggregate results into a summary."""
    similarities = [r["similarity"] for r in results if r.get("similarity") is not None]
    fission_times = [r["fission_sec"] for r in results if r.get("fission_sec")]
    errors = sum(1 for r in results if r.get("fission_has_error"))

    # Checklist aggregation
    chk_ratios = []
    for r in results:
        chk = r.get("fission_checklist", {})
        if chk and chk.get("total", 0) > 0:
            chk_ratios.append(chk["ratio"])

    summary: dict[str, Any] = {
        "total_functions": len(results),
        "compared_functions": len(similarities),
        "fission_errors": errors,
    }

    if similarities:
        sorted_sim = sorted(similarities)
        summary["similarity"] = {
            "avg": round(statistics.fmean(sorted_sim), 2),
            "median": round(sorted_sim[len(sorted_sim)//2], 2),
            "min": round(min(sorted_sim), 2),
            "max": round(max(sorted_sim), 2),
            "p5": round(sorted_sim[max(0, int(len(sorted_sim)*0.05))], 2),
            "p25": round(sorted_sim[max(0, int(len(sorted_sim)*0.25))], 2),
            "p75": round(sorted_sim[min(len(sorted_sim)-1, int(len(sorted_sim)*0.75))], 2),
            "p95": round(sorted_sim[min(len(sorted_sim)-1, int(len(sorted_sim)*0.95))], 2),
        }
        # Distribution buckets
        summary["similarity_distribution"] = {
            ">=90%": sum(1 for s in similarities if s >= 90),
            "70-90%": sum(1 for s in similarities if 70 <= s < 90),
            "50-70%": sum(1 for s in similarities if 50 <= s < 70),
            "<50%": sum(1 for s in similarities if s < 50),
        }

    if chk_ratios:
        summary["checklist"] = {
            "avg_ratio": round(statistics.fmean(chk_ratios), 3),
            "perfect_count": sum(1 for r in chk_ratios if r >= 1.0),
            "total_checked": len(chk_ratios),
        }

    # Category breakdown
    categories: dict[str, list[float]] = {}
    for r in results:
        cat = r.get("category", "unknown")
        if r.get("similarity") is not None:
            categories.setdefault(cat, []).append(r["similarity"])

    summary["by_category"] = {}
    for cat, sims in sorted(categories.items()):
        summary["by_category"][cat] = {
            "count": len(sims),
            "avg_similarity": round(statistics.fmean(sims), 2),
            "min": round(min(sims), 2),
            "max": round(max(sims), 2),
        }

    # Optimization level breakdown
    opt_groups: dict[str, list[float]] = {}
    for r in results:
        opt = r.get("opt_level", "unknown")
        if r.get("similarity") is not None:
            opt_groups.setdefault(opt, []).append(r["similarity"])

    summary["by_opt_level"] = {}
    for opt, sims in sorted(opt_groups.items()):
        summary["by_opt_level"][opt] = {
            "count": len(sims),
            "avg_similarity": round(statistics.fmean(sims), 2),
        }

    # Format breakdown
    fmt_groups: dict[str, list[float]] = {}
    for r in results:
        fmt = r.get("format", "unknown")
        if r.get("similarity") is not None:
            fmt_groups.setdefault(fmt, []).append(r["similarity"])

    summary["by_format"] = {}
    for fmt, sims in sorted(fmt_groups.items()):
        summary["by_format"][fmt] = {
            "count": len(sims),
            "avg_similarity": round(statistics.fmean(sims), 2),
        }

    # Worst 10 functions
    sorted_by_sim = sorted(
        [r for r in results if r.get("similarity") is not None],
        key=lambda r: r["similarity"],
    )
    summary["worst_10"] = [
        {"function": r["function"], "binary": r["binary"],
         "similarity": r["similarity"], "category": r.get("category")}
        for r in sorted_by_sim[:10]
    ]

    return summary


def print_summary(summary: dict) -> None:
    """Print a concise summary to stdout."""
    print("\n" + "=" * 60)
    print(f"  Benchmark: {summary.get('suite', 'unnamed')}")
    print(f"  Functions: {summary.get('total_functions', 0)} total, "
          f"{summary.get('compared_functions', 0)} compared, "
          f"{summary.get('fission_errors', 0)} errors")

    sim = summary.get("similarity", {})
    if sim:
        print(f"\n  Similarity: avg={sim['avg']:.1f}%  median={sim['median']:.1f}%  "
              f"min={sim['min']:.1f}%  max={sim['max']:.1f}%")
        dist = summary.get("similarity_distribution", {})
        print(f"  Distribution: ≥90%={dist.get('>=90%', 0)}  "
              f"70-90%={dist.get('70-90%', 0)}  "
              f"50-70%={dist.get('50-70%', 0)}  "
              f"<50%={dist.get('<50%', 0)}")

    chk = summary.get("checklist", {})
    if chk:
        print(f"\n  Checklist: avg={chk['avg_ratio']:.1%}  "
              f"perfect={chk['perfect_count']}/{chk['total_checked']}")

    by_cat = summary.get("by_category", {})
    if by_cat:
        print("\n  By Category:")
        for cat, info in by_cat.items():
            print(f"    {cat:20s}  avg={info['avg_similarity']:5.1f}%  "
                  f"n={info['count']:3d}  range=[{info['min']:.0f}%, {info['max']:.0f}%]")

    by_opt = summary.get("by_opt_level", {})
    if by_opt:
        print("\n  By Optimization:")
        for opt, info in by_opt.items():
            print(f"    {opt:5s}  avg={info['avg_similarity']:5.1f}%  n={info['count']}")

    by_fmt = summary.get("by_format", {})
    if by_fmt:
        print("\n  By Format:")
        for fmt, info in by_fmt.items():
            print(f"    {fmt:8s}  avg={info['avg_similarity']:5.1f}%  n={info['count']}")

    worst = summary.get("worst_10", [])
    if worst:
        print("\n  Worst 10:")
        for w in worst[:5]:
            print(f"    {w['similarity']:5.1f}%  {w['function']:25s}  [{w.get('category', '')}] {w['binary']}")

    print("=" * 60)

    # CI one-liner
    if sim:
        ci_line = (
            f"[CI] Similarity: {sim['avg']:.1f}% avg "
            f"| {summary.get('compared_functions', 0)} funcs "
            f"| {summary.get('fission_errors', 0)} errors"
        )
        if chk:
            ci_line += f" | Checklist: {chk['avg_ratio']:.1%}"
        print(f"\n{ci_line}")


# ===========================================================================
# CLI
# ===========================================================================

def main() -> int:
    parser = argparse.ArgumentParser(
        description="Fission vs Ghidra decompiler quality benchmark (v4)",
    )
    parser.add_argument("--suite", required=True, help="Suite YAML/JSON file path")
    parser.add_argument("--cache", default=None, help="Ghidra cache directory")
    parser.add_argument("-o", "--output", default=None, help="Output directory for results")
    parser.add_argument("--html", action="store_true", help="Generate HTML report")
    parser.add_argument("--timeout", type=int, default=60, help="Per-function timeout (sec)")
    parser.add_argument("--category", default=None, help="Filter by category (e.g., control)")
    args = parser.parse_args()

    root = _project_root()

    # Resolve suite path
    suite_path = Path(args.suite)
    if not suite_path.is_absolute():
        suite_path = root / suite_path
    if not suite_path.exists():
        suite_path = root / "scripts" / "benchmark" / "suites" / args.suite
    if not suite_path.exists():
        print(f"Error: Suite file not found: {args.suite}")
        return 1

    suite = load_suite(suite_path)

    # Resolve cache + output dirs
    cache_root = Path(args.cache) if args.cache else root / "benchmark_cache"
    ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    output_dir = Path(args.output) if args.output else root / "benchmark_results" / ts

    print(f"Suite: {suite.get('name', 'unnamed')}")
    print(f"Cache: {cache_root}")
    print(f"Output: {output_dir}")
    if args.category:
        print(f"Category filter: {args.category}")

    # Run benchmark
    summary = run_benchmark(suite, cache_root, output_dir, args.timeout, args.category)
    print_summary(summary)

    # HTML report
    if args.html:
        try:
            # Import report module
            sys.path.insert(0, str(Path(__file__).resolve().parent))
            from report import generate_html_report
            results_path = output_dir / "results.json"
            with open(results_path, "r") as f:
                results = json.load(f)
            html_path = output_dir / "report.html"
            generate_html_report(results, summary, html_path)
            print(f"\nHTML report: {html_path}")
        except ImportError:
            print("\nWarning: report.py not found, skipping HTML generation")
        except Exception as e:
            print(f"\nWarning: HTML report failed: {e}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
