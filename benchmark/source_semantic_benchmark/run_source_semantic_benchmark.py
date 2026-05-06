#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = Path(__file__).resolve().parent / "manifests" / "source_owned_all.json"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_ARTIFACT_ROOT = ROOT_DIR / "benchmark" / "artifacts" / "source_semantic_benchmark"

SOURCE_EXTENSIONS = {
    ".c": "c",
    ".cc": "cpp",
    ".cpp": "cpp",
    ".cxx": "cpp",
    ".go": "go",
    ".rs": "rust",
}

CONTROL_WORDS = {
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "return",
    "break",
    "continue",
    "match",
}

CALL_EXCLUDE = CONTROL_WORDS | {
    "sizeof",
    "printf",
    "println",
    "format",
    "vec",
    "make",
    "len",
    "std",
}

INTEGRAL_WORDS = {
    "int",
    "i32",
    "u32",
    "usize",
    "isize",
    "uint",
    "unsigned",
    "signed",
    "long",
    "short",
    "char",
    "bool",
}


@dataclass(frozen=True)
class BenchmarkEntry:
    id: str
    binary_path: Path
    source_path: Path
    language: str
    tags: list[str]
    weight: float = 1.0


@dataclass(frozen=True)
class SourceFunction:
    name: str
    signature: str
    body: str
    return_kind: str
    param_kinds: list[str]
    line: int


@dataclass(frozen=True)
class FissionFunction:
    address: str
    name: str


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT_DIR))
    except ValueError:
        return str(path)


def sanitize_id(text: str) -> str:
    text = re.sub(r"[^A-Za-z0-9_.-]+", "-", text.strip())
    text = text.strip("-._")
    return text or "entry"


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def resolve_path(path: str | Path, root_dir: Path = ROOT_DIR) -> Path:
    p = Path(path)
    return p if p.is_absolute() else root_dir / p


def discover_source_entries(manifest: dict[str, Any]) -> list[BenchmarkEntry]:
    entries: list[BenchmarkEntry] = []

    for raw in manifest.get("entries", []):
        source_path = resolve_path(raw["source_path"])
        binary_path = resolve_path(raw["binary_path"])
        language = raw.get("language") or SOURCE_EXTENSIONS.get(source_path.suffix, "")
        entries.append(
            BenchmarkEntry(
                id=raw.get("id") or sanitize_id(rel(binary_path)),
                binary_path=binary_path,
                source_path=source_path,
                language=language,
                tags=list(raw.get("tags") or []),
                weight=float(raw.get("weight", 1.0) or 1.0),
            )
        )

    for spec in manifest.get("discovery", []):
        root = resolve_path(spec.get("root", "benchmark/binary"))
        languages = set(spec.get("languages") or SOURCE_EXTENSIONS.values())
        require_binary = bool(spec.get("require_binary", True))
        tags = list(spec.get("tags") or [])
        for source_path in sorted(root.rglob("*")):
            if not source_path.is_file():
                continue
            language = SOURCE_EXTENSIONS.get(source_path.suffix)
            if not language or language not in languages:
                continue
            if f"{os.sep}source{os.sep}" not in str(source_path):
                continue
            binary_paths = matching_binary_paths(source_path)
            if require_binary and not binary_paths:
                continue
            if not binary_paths:
                binary_paths = [Path("")]
            for binary_path in binary_paths:
                entry_id = sanitize_id(f"{rel(binary_path)}::{rel(source_path)}")
                entries.append(
                    BenchmarkEntry(
                        id=entry_id,
                        binary_path=binary_path,
                        source_path=source_path,
                        language=language,
                        tags=tags + [language],
                    )
                )

    dedup: dict[tuple[str, str], BenchmarkEntry] = {}
    for entry in entries:
        key = (str(entry.binary_path), str(entry.source_path))
        dedup.setdefault(key, entry)
    return list(dedup.values())


def matching_binary_paths(source_path: Path) -> list[Path]:
    parts = list(source_path.parts)
    try:
        source_idx = parts.index("source")
    except ValueError:
        return []
    if source_idx + 1 >= len(parts):
        return []
    prefix = Path(*parts[:source_idx])
    language_dir = parts[source_idx + 1]
    stem = source_path.stem
    binary_root = prefix / "binary" / language_dir
    if not binary_root.exists():
        return []
    matches: list[Path] = []
    for candidate in sorted(binary_root.rglob("*")):
        if not candidate.is_file():
            continue
        if "_ghidra" in str(candidate):
            continue
        if candidate.name == stem or candidate.stem == stem:
            matches.append(candidate)
    return matches


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m.group(0).count("\n"), text, flags=re.S)
    return re.sub(r"//.*", "", text)


def find_matching_brace(text: str, open_idx: int) -> int | None:
    depth = 0
    i = open_idx
    in_string: str | None = None
    escaped = False
    while i < len(text):
        ch = text[i]
        if in_string:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == in_string:
                in_string = None
        else:
            if ch in ("'", '"'):
                in_string = ch
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return None


def split_params(params: str) -> list[str]:
    params = params.strip()
    if not params or params == "void":
        return []
    result: list[str] = []
    depth = 0
    start = 0
    for i, ch in enumerate(params):
        if ch in "(<[":
            depth += 1
        elif ch in ")>]":
            depth = max(0, depth - 1)
        elif ch == "," and depth == 0:
            result.append(params[start:i].strip())
            start = i + 1
    tail = params[start:].strip()
    if tail:
        result.append(tail)
    return result


def classify_param(param: str, language: str) -> str:
    lowered = param.lower()
    if any(token in lowered for token in ["*", "&", "[]", "slice", "vec", "vector", "["]):
        return "aggregate_or_pointer"
    words = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", lowered))
    if language == "go" and "int" in words:
        return "int"
    if language == "rust" and words.intersection({"i32", "u32", "usize", "isize", "i64", "u64"}):
        return "int"
    if words.intersection(INTEGRAL_WORDS):
        return "int"
    if not words:
        return "unknown"
    return "unsupported"


def classify_return(signature: str, name: str, params: str, language: str) -> str:
    sig = " ".join(signature.strip().split())
    if language == "go":
        after = sig.split(")", 1)[-1].strip()
        return "int" if after == "int" else ("void" if not after else "unsupported")
    if language == "rust":
        m = re.search(r"->\s*([^{]+)$", sig)
        if not m:
            return "void"
        ret = m.group(1).strip().lower()
        return "int" if ret in {"i32", "u32", "usize", "isize", "i64", "u64"} else "unsupported"
    before_name = sig.rsplit(name, 1)[0].strip()
    before_name = re.sub(r"\b(public|private|protected)\s*:\s*", "", before_name)
    if not before_name or before_name.endswith("~"):
        return "void"
    words = set(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", before_name.lower()))
    if "void" in words:
        return "void"
    if words.intersection(INTEGRAL_WORDS):
        return "int"
    return "unsupported"


def extract_source_functions(path: Path, language: str) -> list[SourceFunction]:
    text = path.read_text(encoding="utf-8", errors="replace")
    clean = strip_comments(text)
    if language == "go":
        return extract_go_functions(clean)
    if language == "rust":
        return extract_rust_functions(clean)
    return extract_c_like_functions(clean, language)


def extract_go_functions(text: str) -> list[SourceFunction]:
    pattern = re.compile(
        r"(?m)^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*([^{\n]*)\{"
    )
    funcs: list[SourceFunction] = []
    for match in pattern.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "go"),
                param_kinds=[classify_param(p, "go") for p in split_params(params)],
                line=text.count("\n", 0, match.start()) + 1,
            )
        )
    return funcs


def extract_rust_functions(text: str) -> list[SourceFunction]:
    pattern = re.compile(
        r"(?m)^\s*(?:pub\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]+\"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*(?:->\s*([^{\n]+))?\{"
    )
    funcs: list[SourceFunction] = []
    for match in pattern.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "rust"),
                param_kinds=[classify_param(p, "rust") for p in split_params(params)],
                line=text.count("\n", 0, match.start()) + 1,
            )
        )
    return funcs


def extract_c_like_functions(text: str, language: str) -> list[SourceFunction]:
    funcs: list[SourceFunction] = []
    for open_idx, ch in enumerate(text):
        if ch != "{":
            continue
        prefix = text[:open_idx].rstrip()
        start = max(prefix.rfind(";"), prefix.rfind("}"), prefix.rfind("{")) + 1
        signature = prefix[start:].strip()
        signature = re.sub(r"^\s*(public|private|protected)\s*:\s*", "", signature)
        if not signature or "=" in signature:
            continue
        m = re.search(
            r"([~A-Za-z_][A-Za-z0-9_:~]*)\s*\(([^;{}()]*)\)\s*(?:const)?\s*(?:noexcept)?\s*$",
            signature,
        )
        if not m:
            continue
        name = m.group(1).split("::")[-1]
        if name in {"if", "for", "while", "switch", "catch", "return"}:
            continue
        if re.search(r"\b(class|struct|namespace|enum)\s+$", signature):
            continue
        end = find_matching_brace(text, open_idx)
        if end is None:
            continue
        params = m.group(2)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[open_idx + 1 : end],
                return_kind=classify_return(signature, name, params, language),
                param_kinds=[classify_param(p, language) for p in split_params(params)],
                line=text.count("\n", 0, start) + 1,
            )
        )
    return funcs


def normalize_name(name: str) -> str:
    name = re.sub(r"\s+\[[^\]]+\]\s*$", "", name.strip().lower())
    name = re.sub(r"^(sub_|fun_|_+)", "", name)
    return re.sub(r"[^a-z0-9]+", "", name)


def run_fission_list(binary_path: Path, fission_bin: Path, timeout_sec: int) -> tuple[list[FissionFunction], str | None]:
    cmd = [str(fission_bin), "list", str(binary_path)]
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
        return [], detail.strip() or "list_failed"

    funcs: list[FissionFunction] = []
    for line in res.stdout.splitlines():
        m = re.search(r"(0x[0-9A-Fa-f]+)\s+\d+\s+(.+?)\s*$", line)
        if not m:
            continue
        name = re.sub(r"\s+\[[^\]]+\]\s*$", "", m.group(2).strip()).strip()
        funcs.append(FissionFunction(address=canonical_address(m.group(1)), name=name))
    return funcs, None


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"
    text = str(value).strip()
    if not text:
        return "0x0"
    return f"0x{int(text, 16):x}"


def match_function(source: SourceFunction, funcs: list[FissionFunction]) -> tuple[str, FissionFunction | None, list[str]]:
    src_norm = normalize_name(source.name)
    exact = [f for f in funcs if normalize_name(f.name) == src_norm]
    if len(exact) == 1:
        return "matched", exact[0], []
    if len(exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in exact[:8]]

    suffix = [
        f
        for f in funcs
        if normalize_name(f.name).endswith(src_norm) and src_norm and not normalize_name(f.name).startswith("sub")
    ]
    if len(suffix) == 1:
        return "matched", suffix[0], []
    if len(suffix) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in suffix[:8]]
    return "unmapped", None, []


def parse_json_loose(text: str) -> Any:
    text = text.strip()
    if not text:
        raise json.JSONDecodeError("empty", text, 0)
    starts = [idx for idx in (text.find("["), text.find("{")) if idx >= 0]
    if starts:
        text = text[min(starts) :]
    return json.loads(text)


def run_fission_decomp(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        "decomp",
        str(binary_path),
        "--addr",
        address,
        "--json",
        "--no-header",
        "--no-warnings",
        "--timeout-ms",
        str(max(1000, timeout_sec * 1000)),
    ]
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.TimeoutExpired:
        return {"success": False, "failure_kind": "timeout", "wall_sec": round(time.perf_counter() - start, 6)}
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or str(exc)).strip()
        return {
            "success": False,
            "failure_kind": "command_failed",
            "failure_detail": detail[-4000:],
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    try:
        payload = parse_json_loose(res.stdout)
    except json.JSONDecodeError as exc:
        return {
            "success": False,
            "failure_kind": "invalid_json",
            "failure_detail": str(exc),
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    if isinstance(payload, list):
        func = payload[0] if payload else {}
    else:
        func = (payload.get("functions") or [{}])[0] if isinstance(payload, dict) else {}
    if func.get("error"):
        return {
            "success": False,
            "failure_kind": "decompile_error",
            "failure_detail": func.get("error"),
            "wall_sec": round(time.perf_counter() - start, 6),
            "engine_used": func.get("engine_used"),
        }
    code = func.get("code") or ""
    if not code.strip():
        return {"success": False, "failure_kind": "empty_output", "wall_sec": round(time.perf_counter() - start, 6)}
    return {
        "success": True,
        "code": code,
        "wall_sec": round(time.perf_counter() - start, 6),
        "engine_used": func.get("engine_used"),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "preview_build_stats": func.get("preview_build_stats"),
    }


def code_fingerprint(code: str, func: SourceFunction | None = None) -> Counter[str]:
    stripped = strip_comments(code)
    counter: Counter[str] = Counter()
    for word in re.findall(r"\b[A-Za-z_][A-Za-z0-9_]*\b", stripped):
        lowered = word.lower()
        if lowered in CONTROL_WORDS:
            counter[f"ctrl:{lowered}"] += 1
    for op in ["<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "->", "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "="]:
        counter[f"op:{op}"] += stripped.count(op)
    for const in re.findall(r"\b(?:0x[0-9A-Fa-f]+|\d+)\b", stripped):
        counter[f"const:{const.lower()}"] += 1
    for call in re.findall(r"\b([A-Za-z_][A-Za-z0-9_:]*)\s*\(", stripped):
        lowered = call.split("::")[-1].lower()
        if lowered not in CALL_EXCLUDE:
            counter[f"call:{normalize_name(lowered)}"] += 1
    counter["mem:index"] += stripped.count("[")
    counter["mem:deref_or_ptr"] += stripped.count("*")
    counter["mem:field"] += stripped.count("->") + stripped.count(".")
    if func is not None:
        counter[f"sig:return:{func.return_kind}"] += 1
        counter[f"sig:param_count:{len(func.param_kinds)}"] += 1
        for kind in func.param_kinds:
            counter[f"sig:param:{kind}"] += 1
    return +counter


def multiset_jaccard(left: Counter[str], right: Counter[str]) -> float:
    keys = set(left) | set(right)
    if not keys:
        return 1.0
    inter = sum(min(left[k], right[k]) for k in keys)
    union = sum(max(left[k], right[k]) for k in keys)
    return round(inter / union, 6) if union else 1.0


def behavior_cases(param_count: int) -> list[tuple[int, ...]]:
    if param_count == 0:
        return [()]
    if param_count == 1:
        return [(0,), (1,), (2,), (5,), (10,), (-1,)]
    if param_count == 2:
        return [(0, 0), (1, 2), (5, 10), (-3, 7), (42, -1)]
    if param_count == 3:
        return [(1, 2, 3), (0, 5, -1), (7, 3, 2)]
    return []


def behavior_supported(func: SourceFunction, language: str) -> tuple[bool, str | None]:
    if language != "c":
        return False, "dynamic harness currently supports C source functions only"
    if func.name == "main":
        return False, "main is not called as a unit function"
    if func.return_kind != "int":
        return False, f"unsupported return kind: {func.return_kind}"
    if any(kind != "int" for kind in func.param_kinds):
        return False, f"unsupported parameter kinds: {func.param_kinds}"
    if not behavior_cases(len(func.param_kinds)):
        return False, "unsupported arity"
    return True, None


def source_harness(source_path: Path, func: SourceFunction, cases: list[tuple[int, ...]]) -> str:
    calls = "\n".join(render_case_call(func.name, case) for case in cases)
    return f"""
#include <stdio.h>
#define main source_original_main
#include "{source_path}"
#undef main
int main(void) {{
{calls}
    return 0;
}}
"""


def candidate_harness(candidate_code: str, func: SourceFunction, cases: list[tuple[int, ...]]) -> str:
    calls = "\n".join(render_case_call(func.name, case) for case in cases)
    return f"""
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
typedef unsigned char byte;
typedef unsigned char uchar;
typedef signed char schar;
typedef unsigned char undefined1;
typedef unsigned short ushort;
typedef unsigned short word;
typedef unsigned short undefined2;
typedef unsigned int uint;
typedef unsigned long ulong;
typedef unsigned int dword;
typedef unsigned int undefined4;
typedef unsigned long long ulonglong;
typedef unsigned long long qword;
typedef unsigned long long undefined8;
typedef long long longlong;
typedef unsigned char uint8;
typedef unsigned short uint16;
typedef unsigned int uint32;
typedef unsigned long long uint64;
typedef signed char int8;
typedef short int16;
typedef int int32;
typedef long long int64;
{candidate_code}
int main(void) {{
{calls}
    return 0;
}}
"""


def render_case_call(name: str, case: tuple[int, ...]) -> str:
    args = ", ".join(str(v) for v in case)
    return f'    printf("%lld\\n", (long long){name}({args}));'


def compile_and_run_c(code: str, cwd: Path, name: str, timeout_sec: int) -> dict[str, Any]:
    source = cwd / f"{name}.c"
    binary = cwd / name
    source.write_text(code, encoding="utf-8")
    clang = os.environ.get("CLANG") or shutil.which("clang") or "/opt/homebrew/opt/llvm/bin/clang"
    cmd = [clang, "-x", "c", "-std=c11", "-Wno-everything", str(source), "-o", str(binary)]
    try:
        compile_res = subprocess.run(
            cmd,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        return {
            "status": "compile_failed",
            "detail": (exc.stderr or exc.stdout or str(exc))[-4000:],
        }
    except subprocess.TimeoutExpired:
        return {"status": "compile_timeout"}

    try:
        run_res = subprocess.run(
            [str(binary)],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        return {"status": "run_failed", "detail": (exc.stderr or exc.stdout or str(exc))[-4000:]}
    except subprocess.TimeoutExpired:
        return {"status": "run_timeout"}

    return {"status": "ok", "stdout": run_res.stdout, "compile_stdout": compile_res.stdout}


def c_host_execution_probe(timeout_sec: int) -> dict[str, Any]:
    code = """
#include <stdio.h>
int main(void) {
    puts("source-semantic-host-ok");
    return 0;
}
"""
    with tempfile.TemporaryDirectory(prefix="source-semantic-host-") as tmp:
        result = compile_and_run_c(code, Path(tmp), "host_probe", timeout_sec)
    if result.get("status") == "ok" and result.get("stdout", "").strip() == "source-semantic-host-ok":
        return {"status": "ok"}
    return {
        "status": f"host_{result.get('status', 'unknown')}",
        "detail": result.get("detail"),
    }


def run_behavior_check(
    entry: BenchmarkEntry,
    func: SourceFunction,
    decomp_code: str | None,
    timeout_sec: int,
    host_execution: dict[str, Any],
) -> dict[str, Any]:
    supported, reason = behavior_supported(func, entry.language)
    if not supported:
        return {"status": "unsupported_signature", "score": 0.0, "reason": reason}
    if host_execution.get("status") != "ok":
        return {
            "status": "host_execution_unavailable",
            "score": 0.0,
            "reason": host_execution.get("status"),
            "detail": host_execution.get("detail"),
        }
    if not decomp_code:
        return {"status": "decomp_failed", "score": 0.0}

    cases = behavior_cases(len(func.param_kinds))
    with tempfile.TemporaryDirectory(prefix="source-semantic-") as tmp:
        tmp_path = Path(tmp)
        oracle = compile_and_run_c(source_harness(entry.source_path, func, cases), tmp_path, "oracle", timeout_sec)
        if oracle.get("status") != "ok":
            return {"status": f"oracle_{oracle.get('status')}", "score": 0.0, "detail": oracle.get("detail")}
        candidate = compile_and_run_c(candidate_harness(decomp_code, func, cases), tmp_path, "candidate", timeout_sec)
        if candidate.get("status") != "ok":
            return {"status": f"candidate_{candidate.get('status')}", "score": 0.0, "detail": candidate.get("detail")}
        oracle_lines = [line.strip() for line in oracle["stdout"].splitlines() if line.strip()]
        candidate_lines = [line.strip() for line in candidate["stdout"].splitlines() if line.strip()]
        passed = oracle_lines == candidate_lines
        return {
            "status": "pass" if passed else "mismatch",
            "score": 1.0 if passed else 0.0,
            "cases": [list(case) for case in cases],
            "oracle": oracle_lines,
            "candidate": candidate_lines,
        }


def summarize(rows: list[dict[str, Any]], manifest_name: str, entries: list[BenchmarkEntry]) -> dict[str, Any]:
    total = len(rows)
    mapped = sum(1 for row in rows if row["mapping_status"] == "matched")
    decomp_ok = sum(1 for row in rows if row.get("decomp_success"))
    compile_ok = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    behavior_pass = sum(1 for row in rows if row.get("behavior", {}).get("status") == "pass")
    score_values = [float(row.get("semantic_score", 0.0) or 0.0) for row in rows]
    by_language: dict[str, dict[str, Any]] = {}
    for row in rows:
        lang = row["language"]
        bucket = by_language.setdefault(
            lang,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        bucket["row_count"] += 1
        bucket["mapped"] += int(row["mapping_status"] == "matched")
        bucket["decomp_success"] += int(bool(row.get("decomp_success")))
        bucket["behavior_pass"] += int(row.get("behavior", {}).get("status") == "pass")
        bucket["score_sum"] += float(row.get("semantic_score", 0.0) or 0.0)
    for bucket in by_language.values():
        count = max(1, bucket["row_count"])
        bucket["avg_semantic_score"] = round(bucket.pop("score_sum") / count, 6)
    host_statuses = Counter(
        row.get("behavior", {}).get("reason")
        for row in rows
        if row.get("behavior", {}).get("status") == "host_execution_unavailable"
    )
    return {
        "manifest": manifest_name,
        "entry_count": len(entries),
        "row_count": total,
        "function_mapping_rate": round(mapped / total, 6) if total else 0.0,
        "decomp_success_rate": round(decomp_ok / total, 6) if total else 0.0,
        "candidate_compile_rate": round(compile_ok / total, 6) if total else 0.0,
        "behavior_pass_rate": round(behavior_pass / total, 6) if total else 0.0,
        "weighted_semantic_similarity": round(sum(score_values) / total, 6) if total else 0.0,
        "host_execution_unavailable_count": sum(host_statuses.values()),
        "host_execution_unavailable_reasons": dict(host_statuses),
        "by_language": by_language,
    }


def render_markdown(summary: dict[str, Any], rows: list[dict[str, Any]]) -> str:
    lines = [
        f"# Source Semantic Benchmark: {summary['manifest']}",
        "",
        f"- Entries: {summary['entry_count']}",
        f"- Rows: {summary['row_count']}",
        f"- Function mapping rate: {summary['function_mapping_rate']:.3f}",
        f"- Decompile success rate: {summary['decomp_success_rate']:.3f}",
        f"- Candidate compile rate: {summary['candidate_compile_rate']:.3f}",
        f"- Behavior pass rate: {summary['behavior_pass_rate']:.3f}",
        f"- Weighted semantic similarity: {summary['weighted_semantic_similarity']:.3f}",
        f"- Host execution unavailable rows: {summary['host_execution_unavailable_count']}",
        "",
        "## By Language",
        "",
        "| Language | Rows | Mapped | Decomp OK | Behavior Pass | Avg Score |",
        "|---|---:|---:|---:|---:|---:|",
    ]
    for lang, bucket in sorted(summary["by_language"].items()):
        lines.append(
            f"| {lang} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
            f"{bucket['behavior_pass']} | {bucket['avg_semantic_score']:.3f} |"
        )
    failing = [row for row in rows if row.get("semantic_score", 0.0) < 1.0][:20]
    if failing:
        lines.extend(["", "## First Non-Perfect Rows", ""])
        for row in failing:
            lines.append(
                f"- `{row['entry_id']}` `{row['function_name']}`: score={row['semantic_score']:.3f}, "
                f"map={row['mapping_status']}, behavior={row.get('behavior', {}).get('status')}"
            )
    lines.append("")
    return "\n".join(lines)


def row_for_function(
    entry: BenchmarkEntry,
    func: SourceFunction,
    fission_funcs: list[FissionFunction],
    fission_error: str | None,
    fission_bin: Path,
    timeout_sec: int,
    host_execution: dict[str, Any],
) -> dict[str, Any]:
    source_fp = code_fingerprint(func.body, func)
    mapping_status, matched, candidates = match_function(func, fission_funcs) if not fission_error else ("list_failed", None, [])
    decomp: dict[str, Any] = {"success": False, "failure_kind": mapping_status}
    if matched is not None:
        decomp = run_fission_decomp(entry.binary_path, matched.address, fission_bin, timeout_sec)
    decomp_code = decomp.get("code") if decomp.get("success") else None
    decomp_fp = code_fingerprint(decomp_code or "") if decomp_code else Counter()
    static_score = multiset_jaccard(source_fp, decomp_fp) if decomp_code else 0.0
    behavior = run_behavior_check(entry, func, decomp_code, timeout_sec, host_execution)
    semantic_score = round(0.65 * float(behavior.get("score", 0.0)) + 0.35 * static_score, 6)
    return {
        "entry_id": entry.id,
        "binary_path": rel(entry.binary_path),
        "source_path": rel(entry.source_path),
        "language": entry.language,
        "tags": entry.tags,
        "function_name": func.name,
        "source_line": func.line,
        "source_signature": func.signature,
        "source_return_kind": func.return_kind,
        "source_param_kinds": func.param_kinds,
        "address": matched.address if matched else None,
        "fission_name": matched.name if matched else None,
        "mapping_status": mapping_status,
        "mapping_candidates": candidates,
        "list_error": fission_error,
        "decomp_success": bool(decomp.get("success")),
        "decomp_failure_kind": decomp.get("failure_kind"),
        "decomp_failure_detail": decomp.get("failure_detail"),
        "engine_used": decomp.get("engine_used"),
        "static_semantic_score": static_score,
        "behavior": behavior,
        "semantic_score": semantic_score,
    }


def run_benchmark(args: argparse.Namespace) -> int:
    manifest_path = resolve_path(args.manifest)
    manifest = load_json(manifest_path)
    entries = discover_source_entries(manifest)
    if args.limit_binaries is not None:
        entries = entries[: args.limit_binaries]

    output_dir = resolve_path(args.output_dir) if args.output_dir else DEFAULT_ARTIFACT_ROOT / f"{manifest.get('name', 'source-semantic')}-latest"
    output_dir.mkdir(parents=True, exist_ok=True)
    fission_bin = resolve_path(args.fission_bin)
    host_execution = c_host_execution_probe(args.timeout_sec)

    rows: list[dict[str, Any]] = []
    for entry in entries:
        source_functions = extract_source_functions(entry.source_path, entry.language)
        if args.limit_functions is not None:
            source_functions = source_functions[: args.limit_functions]
        fission_funcs, fission_error = run_fission_list(entry.binary_path, fission_bin, args.timeout_sec)
        for func in source_functions:
            rows.append(
                row_for_function(
                    entry,
                    func,
                    fission_funcs,
                    fission_error,
                    fission_bin,
                    args.timeout_sec,
                    host_execution,
                )
            )

    summary = summarize(rows, manifest.get("name", manifest_path.stem), entries)
    (output_dir / "source_semantic_rows.json").write_text(
        json.dumps(rows, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (output_dir / "source_semantic_summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    (output_dir / "source_semantic_summary.md").write_text(render_markdown(summary, rows), encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


def run_self_test() -> int:
    sample = """
int add(int a, int b) { return a + b; }
int max(int a, int b) { if (a > b) return a; return b; }
"""
    with tempfile.TemporaryDirectory(prefix="source-semantic-selftest-") as tmp:
        path = Path(tmp) / "sample.c"
        path.write_text(sample, encoding="utf-8")
        funcs = extract_source_functions(path, "c")
        assert [f.name for f in funcs] == ["add", "max"]
        assert funcs[0].return_kind == "int"
        assert funcs[0].param_kinds == ["int", "int"]
        assert multiset_jaccard(code_fingerprint(funcs[0].body, funcs[0]), code_fingerprint(funcs[0].body, funcs[0])) == 1.0
        status, matched, _ = match_function(funcs[0], [FissionFunction("0x1000", "add [export]")])
        assert status == "matched"
        assert matched is not None
    print("self-test ok")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark Fission pseudocode against original source semantics. Ghidra is not used."
    )
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST), help="Source semantic manifest JSON")
    parser.add_argument("--fission-bin", default=str(DEFAULT_FISSION_BIN), help="Path to fission_cli")
    parser.add_argument("--output-dir", help="Output artifact directory")
    parser.add_argument("--limit-binaries", type=int, help="Limit discovered manifest entries")
    parser.add_argument("--limit-functions", type=int, help="Limit source functions per entry")
    parser.add_argument("--timeout-sec", type=int, default=30, help="Per-command timeout")
    parser.add_argument("--self-test", action="store_true", help="Run lightweight parser/scoring self-test")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_benchmark(args)


if __name__ == "__main__":
    raise SystemExit(main())
