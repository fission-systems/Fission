#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import orjson
except ImportError:  # pragma: no cover - optional fast path
    orjson = None


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = Path(__file__).resolve().parent / "manifests" / "source_owned_all.json"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_ARTIFACT_ROOT = ROOT_DIR / "benchmark" / "artifacts" / "source_semantic_benchmark"
DEFAULT_DECOMP_CACHE_FILE = DEFAULT_ARTIFACT_ROOT / ".cache" / "decomp_cache.json"
DEFAULT_LIST_CACHE_FILE = DEFAULT_ARTIFACT_ROOT / ".cache" / "list_cache.json"
DEFAULT_HISTORY_FILE = DEFAULT_ARTIFACT_ROOT / "source_semantic_history.jsonl"
DEFAULT_LATEST_INDEX_FILE = DEFAULT_ARTIFACT_ROOT / "source_semantic_latest_by_manifest.json"
DEFAULT_JOBS = max(1, (os.cpu_count() or 2) // 2)

SANITIZE_ID_RE = re.compile(r"[^A-Za-z0-9_.-]+")
BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.S)
LINE_COMMENT_RE = re.compile(r"//.*")
WORD_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
WORD_BOUNDARY_RE = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
CTYPE_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_\s*]*")
CONST_RE = re.compile(r"\b(?:0x[0-9A-Fa-f]+|\d+)\b")
CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_:]*)\s*\(")
ARRAY_SUFFIX_RE = re.compile(r"\[[^\]]*\]")
RETURN_ARROW_RE = re.compile(r"->\s*([^{]+)$")
ACCESS_LABEL_RE = re.compile(r"\b(public|private|protected)\s*:\s*")
C_LIKE_ACCESS_PREFIX_RE = re.compile(r"^\s*(public|private|protected)\s*:\s*")
C_LIKE_FUNCTION_RE = re.compile(
    r"([~A-Za-z_][A-Za-z0-9_:~]*)\s*\(([^;{}()]*)\)\s*(?:const)?\s*(?:noexcept)?\s*$"
)
C_LIKE_TYPE_DECL_RE = re.compile(r"\b(class|struct|namespace|enum)\s+$")
TRAILING_DECORATION_RE = re.compile(r"\s+\[[^\]]+\]\s*$")
NORMALIZE_PREFIX_RE = re.compile(r"^(sub_|fun_|_+)")
NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")
FISSION_LIST_LINE_RE = re.compile(r"(0x[0-9A-Fa-f]+)\s+\d+\s+(.+?)\s*$")
GO_FUNCTION_RE = re.compile(
    r"(?m)^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*([^{\n]*)\{"
)
RUST_FUNCTION_RE = re.compile(
    r"(?m)^\s*(?:pub\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]+\"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*(?:->\s*([^{\n]+))?\{"
)

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
    "i8",
    "i16",
    "i32",
    "i64",
    "u32",
    "u64",
    "usize",
    "isize",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "uint",
    "unsigned",
    "signed",
    "long",
    "short",
    "char",
    "bool",
}

UNSIGNED_INTEGRAL_WORDS = {
    "u8",
    "u16",
    "u32",
    "u64",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "uint",
    "usize",
    "unsigned",
}


@dataclass(frozen=True)
class BenchmarkEntry:
    id: str
    binary_path: Path
    source_path: Path
    language: str
    tags: list[str]
    weight: float = 1.0
    behavior_cases: dict[str, list[dict[str, Any]]] | None = None


@dataclass(frozen=True)
class SourceFunction:
    name: str
    signature: str
    body: str
    return_kind: str
    param_kinds: list[str]
    param_names: list[str]
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
    text = SANITIZE_ID_RE.sub("-", text.strip())
    text = text.strip("-._")
    return text or "entry"


def utc_now() -> datetime.datetime:
    return datetime.datetime.now(datetime.UTC)


def utc_timestamp_slug(now: datetime.datetime) -> str:
    return now.strftime("%Y%m%dT%H%M%SZ")


def utc_isoformat(now: datetime.datetime) -> str:
    return now.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def load_json(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if orjson is not None:
        return orjson.loads(data)
    return json.loads(data.decode("utf-8"))


def dump_json_pretty(value: Any) -> str:
    if orjson is not None:
        return orjson.dumps(value, option=orjson.OPT_INDENT_2 | orjson.OPT_SORT_KEYS).decode("utf-8") + "\n"
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def dump_json_line(value: Any) -> str:
    if orjson is not None:
        return orjson.dumps(value, option=orjson.OPT_SORT_KEYS).decode("utf-8") + "\n"
    return json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"


def load_json_list_or_dict(path: Path) -> Any:
    data = path.read_bytes()
    if orjson is not None:
        return orjson.loads(data)
    return json.loads(data.decode("utf-8"))


def resolve_path(path: str | Path, root_dir: Path = ROOT_DIR) -> Path:
    p = Path(path)
    return p if p.is_absolute() else root_dir / p


def file_cache_fingerprint(path: Path) -> str:
    try:
        resolved = path.resolve()
        stat = resolved.stat()
    except OSError:
        return f"{path}:missing"
    return f"{resolved}:size={stat.st_size}:mtime_ns={stat.st_mtime_ns}"


def decomp_cache_key(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    include_debug_decomp: bool,
) -> str:
    return "|".join(
        [
            "source-semantic-decomp-v1",
            f"binary={file_cache_fingerprint(binary_path)}",
            f"fission_bin={file_cache_fingerprint(fission_bin)}",
            f"addr={canonical_address(address)}",
            f"debug={int(include_debug_decomp)}",
        ]
    )


def list_cache_key(binary_path: Path, fission_bin: Path) -> str:
    return "|".join(
        [
            "source-semantic-list-v1",
            f"binary={file_cache_fingerprint(binary_path)}",
            f"fission_bin={file_cache_fingerprint(fission_bin)}",
        ]
    )


def load_decomp_cache(path: Path | None) -> dict[str, dict[str, Any]]:
    if path is None or not path.exists():
        return {}
    try:
        raw = load_json_list_or_dict(path)
    except Exception:
        return {}
    if not isinstance(raw, dict):
        return {}
    entries = raw.get("entries", raw)
    if not isinstance(entries, dict):
        return {}
    return {str(key): value for key, value in entries.items() if isinstance(value, dict)}


def save_decomp_cache(path: Path | None, cache: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "source-semantic-decomp-cache-v1",
        "updated_at_unix": round(time.time(), 6),
        "entry_count": len(cache),
        "entries": cache,
    }
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(dump_json_pretty(payload), encoding="utf-8")
    tmp_path.replace(path)


def load_list_cache(path: Path | None) -> dict[str, dict[str, Any]]:
    return load_decomp_cache(path)


def save_list_cache(path: Path | None, cache: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "source-semantic-list-cache-v1",
        "updated_at_unix": round(time.time(), 6),
        "entry_count": len(cache),
        "entries": cache,
    }
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(dump_json_pretty(payload), encoding="utf-8")
    tmp_path.replace(path)


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
                behavior_cases=raw.get("behavior_cases"),
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
                        behavior_cases=spec.get("behavior_cases"),
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
    text = BLOCK_COMMENT_RE.sub(lambda m: "\n" * m.group(0).count("\n"), text)
    return LINE_COMMENT_RE.sub("", text)


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
    words = set(WORD_RE.findall(lowered))
    if language in {"c", "cpp"} and any(token in lowered for token in ["*", "[]", "["]):
        if words.intersection(INTEGRAL_WORDS):
            return "int_ptr"
        return "aggregate_or_pointer"
    if any(token in lowered for token in ["*", "&", "[]", "slice", "vec", "vector", "["]):
        return "aggregate_or_pointer"
    if language == "go" and "int" in words:
        return "int"
    if language == "rust" and words.intersection({"u32", "usize", "u64"}):
        return "uint"
    if language == "rust" and words.intersection({"i32", "isize", "i64"}):
        return "int"
    if language in {"c", "cpp"} and words.intersection(UNSIGNED_INTEGRAL_WORDS):
        return "uint"
    if words.intersection(INTEGRAL_WORDS):
        return "int"
    if not words:
        return "unknown"
    return "unsupported"


def param_name(param: str, index: int) -> str:
    cleaned = ARRAY_SUFFIX_RE.sub("", param)
    words = WORD_RE.findall(cleaned)
    type_words = {
        "const",
        "volatile",
        "restrict",
        "signed",
        "unsigned",
        "short",
        "long",
        "int",
        "char",
        "void",
        "bool",
        "static",
    }
    for word in reversed(words):
        if word.lower() not in type_words:
            return word
    return f"param_{index + 1}"


def param_names(params: str) -> list[str]:
    return [param_name(param, index) for index, param in enumerate(split_params(params))]


def classify_return(signature: str, name: str, params: str, language: str) -> str:
    sig = " ".join(signature.strip().split())
    if language == "go":
        after = sig.split(")", 1)[-1].strip()
        return "int" if after == "int" else ("void" if not after else "unsupported")
    if language == "rust":
        m = RETURN_ARROW_RE.search(sig)
        if not m:
            return "void"
        ret = m.group(1).strip().lower()
        return "int" if ret in {"i32", "u32", "usize", "isize", "i64", "u64"} else "unsupported"
    before_name = sig.rsplit(name, 1)[0].strip()
    before_name = ACCESS_LABEL_RE.sub("", before_name)
    if not before_name or before_name.endswith("~"):
        return "void"
    words = set(WORD_RE.findall(before_name.lower()))
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
    funcs: list[SourceFunction] = []
    for match in GO_FUNCTION_RE.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "go"),
                param_kinds=[classify_param(p, "go") for p in params_split],
                param_names=param_names(params),
                line=text.count("\n", 0, match.start()) + 1,
            )
        )
    return funcs


def extract_rust_functions(text: str) -> list[SourceFunction]:
    funcs: list[SourceFunction] = []
    for match in RUST_FUNCTION_RE.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "rust"),
                param_kinds=[classify_param(p, "rust") for p in params_split],
                param_names=param_names(params),
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
        signature = C_LIKE_ACCESS_PREFIX_RE.sub("", signature)
        if not signature or "=" in signature:
            continue
        m = C_LIKE_FUNCTION_RE.search(signature)
        if not m:
            continue
        name = m.group(1).split("::")[-1]
        if name in {"if", "for", "while", "switch", "catch", "return"}:
            continue
        if C_LIKE_TYPE_DECL_RE.search(signature):
            continue
        end = find_matching_brace(text, open_idx)
        if end is None:
            continue
        params = m.group(2)
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[open_idx + 1 : end],
                return_kind=classify_return(signature, name, params, language),
                param_kinds=[classify_param(p, language) for p in params_split],
                param_names=param_names(params),
                line=text.count("\n", 0, start) + 1,
            )
        )
    return funcs


def normalize_name(name: str) -> str:
    name = TRAILING_DECORATION_RE.sub("", name.strip().lower())
    name = NORMALIZE_PREFIX_RE.sub("", name)
    return NON_ALNUM_RE.sub("", name)


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
        m = FISSION_LIST_LINE_RE.search(line)
        if not m:
            continue
        name = TRAILING_DECORATION_RE.sub("", m.group(2).strip()).strip()
        funcs.append(FissionFunction(address=canonical_address(m.group(1)), name=name))
    return funcs, None


def run_fission_list_cached(
    binary_path: Path,
    fission_bin: Path,
    timeout_sec: int,
    cache: dict[str, dict[str, Any]],
    cache_stats: Counter[str],
) -> tuple[list[FissionFunction], str | None]:
    key = list_cache_key(binary_path, fission_bin)
    cached = cache.get(key)
    if cached is not None:
        cache_stats["hit"] += 1
        funcs = [
            FissionFunction(address=str(raw.get("address")), name=str(raw.get("name")))
            for raw in cached.get("functions", [])
            if isinstance(raw, dict) and raw.get("address") and raw.get("name")
        ]
        error = cached.get("error")
        return funcs, str(error) if error else None

    cache_stats["miss"] += 1
    funcs, error = run_fission_list(binary_path, fission_bin, timeout_sec)
    cache[key] = {
        "functions": [{"address": func.address, "name": func.name} for func in funcs],
        "error": error,
    }
    cache_stats["stored"] += 1
    return funcs, error


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"
    text = str(value).strip()
    if not text:
        return "0x0"
    return f"0x{int(text, 16):x}"


def match_function(source: SourceFunction, funcs: list[FissionFunction]) -> tuple[str, FissionFunction | None, list[str]]:
    literal_exact = [f for f in funcs if f.name == source.name]
    if len(literal_exact) == 1:
        return "matched", literal_exact[0], []
    if len(literal_exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in literal_exact[:8]]

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


def select_source_functions(
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    limit: int | None,
    fission_error: str | None = None,
) -> list[SourceFunction]:
    if limit is None:
        return source_functions
    if limit <= 0:
        return []
    if fission_error or not fission_funcs:
        return source_functions[:limit]

    matched: list[SourceFunction] = []
    fallback: list[SourceFunction] = []
    for func in source_functions:
        status, matched_func, _ = match_function(func, fission_funcs)
        if status == "matched" and matched_func is not None:
            matched.append(func)
        else:
            fallback.append(func)
    return (matched + fallback)[:limit]


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
    include_debug_decomp: bool = False,
    debug_decomp_bundle_path: Path | None = None,
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
    if include_debug_decomp:
        cmd.append("--debug-decomp")
    if debug_decomp_bundle_path is not None:
        debug_decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        cmd.extend(["--debug-decomp-bundle", str(debug_decomp_bundle_path)])
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
            "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
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
        "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
        "debug_decomp_bundle_path": rel(debug_decomp_bundle_path)
        if debug_decomp_bundle_path is not None
        else None,
    }


def debug_decomp_summary(debug_decomp: Any) -> dict[str, Any] | None:
    if not isinstance(debug_decomp, dict):
        return None
    quality = debug_decomp.get("quality_evidence") if isinstance(debug_decomp.get("quality_evidence"), dict) else {}
    pipeline = (
        debug_decomp.get("rust_sleigh_pipeline")
        if isinstance(debug_decomp.get("rust_sleigh_pipeline"), dict)
        else {}
    )
    pcode_blocks = pipeline.get("raw_pcode_blocks") if isinstance(pipeline.get("raw_pcode_blocks"), list) else []
    sampled_pcode_blocks = pcode_blocks[:64]
    return {
        "stage_status": debug_decomp.get("stage_status"),
        "stage_metrics": debug_decomp.get("stage_metrics"),
        "owner_buckets": debug_decomp.get("owner_buckets") or [],
        "rust_sleigh_pipeline": {
            key: pipeline.get(key)
            for key in [
                "entry_address",
                "max_bytes",
                "instruction_limit",
                "decode_attempt_count",
                "decode_stop_reason",
                "template_source_counts",
                "raw_pcode_block_count",
                "raw_pcode_op_count",
                "raw_pcode_edge_count",
                "raw_pcode_terminal_opcode_counts",
                "raw_pcode_block_evidence_truncated",
                "strict_indirect_retry_attempted",
                "nir_fallback_kind",
                "nir_fallback_kind_refined",
                "nir_fallback_reason_summary",
                "pipeline_stage_status",
            ]
            if key in pipeline
        } | (
            {
                "raw_pcode_blocks_sampled_count": len(sampled_pcode_blocks),
                "raw_pcode_blocks": sampled_pcode_blocks,
            }
            if sampled_pcode_blocks
            else {}
        ),
        "quality_evidence": {
            key: quality.get(key)
            for key in [
                "validated_pcode_op_count",
                "invalid_pcode_shape_count",
                "replacement_plan_rejected_missing_merge_count",
                "replacement_plan_rejected_alias_unsafe_count",
                "forced_linear_structuring_count",
                "structuring_irreducible_scc_count",
                "region_emit_ready_failed_count",
                "call_target_unresolved_sub_fallback_count",
                "call_prototype_signature_missing_count",
                "typed_fact_conflict_count",
            ]
            if key in quality
        },
    }


def run_fission_decomp_cached(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    debug_decomp_bundle_path: Path | None,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> dict[str, Any]:
    key = decomp_cache_key(binary_path, address, fission_bin, include_debug_decomp)
    with cache_lock:
        cached = cache.get(key)
        if cached is not None:
            cache_stats["hit"] += 1
    if cached is not None:
        cached_result = dict(cached)
        cached_result["decomp_cache_status"] = "hit"
        if (
            include_debug_decomp
            and debug_decomp_bundle_path is not None
            and not debug_decomp_bundle_path.exists()
        ):
            cached_result = run_fission_decomp(
                binary_path,
                address,
                fission_bin,
                timeout_sec,
                include_debug_decomp=include_debug_decomp,
                debug_decomp_bundle_path=debug_decomp_bundle_path,
            )
            cached_result["decomp_cache_status"] = "refreshed_debug_bundle"
            with cache_lock:
                cache[key] = cached_result
                cache_stats["stored"] += 1
        elif debug_decomp_bundle_path is not None:
            cached_result["debug_decomp_bundle_path"] = rel(debug_decomp_bundle_path)
        return cached_result
    with cache_lock:
        cache_stats["miss"] += 1
    decomp = run_fission_decomp(
        binary_path,
        address,
        fission_bin,
        timeout_sec,
        include_debug_decomp=include_debug_decomp,
        debug_decomp_bundle_path=debug_decomp_bundle_path,
    )
    decomp["decomp_cache_status"] = "miss"
    with cache_lock:
        cache.setdefault(key, decomp)
        cache_stats["stored"] += 1
    return dict(decomp)


def shell_command(parts: list[Any]) -> str:
    return " ".join(shlex.quote(str(part)) for part in parts)


def debug_bundle_path_for_parts(
    output_dir: Path,
    entry_id: str | None,
    function_name: str | None,
    address: str | None,
) -> Path:
    entry = sanitize_id(str(entry_id or "entry"))
    function = sanitize_id(str(function_name or "function"))
    address = sanitize_id(str(address or "no-address"))
    return output_dir / "debug_decomp" / entry / f"{function}-{address}.json"


def debug_bundle_path_for_row(output_dir: Path, row: dict[str, Any]) -> Path:
    return debug_bundle_path_for_parts(
        output_dir,
        row.get("entry_id"),
        row.get("function_name"),
        row.get("address"),
    )


def debug_triage_path_for_row(output_dir: Path, row: dict[str, Any], kind: str, suffix: str) -> Path:
    stem = "-".join(
        [
            sanitize_id(str(row.get("entry_id") or "entry")),
            sanitize_id(str(row.get("function_name") or "function")),
            sanitize_id(str(row.get("address") or "unknown")),
        ]
    )
    return output_dir / "debug_triage" / kind / f"{stem}.{suffix}"


def behavior_artifact_dir_for_row(
    output_dir: Path,
    entry: BenchmarkEntry,
    func: SourceFunction,
    address: str | None,
) -> Path:
    entry_id = sanitize_id(entry.id)
    function = sanitize_id(func.name)
    address_id = sanitize_id(address or "no-address")
    return output_dir / "behavior" / entry_id / f"{function}-{address_id}"


def write_behavior_artifacts(
    artifact_dir: Path,
    oracle_code: str,
    candidate_code: str,
    oracle: dict[str, Any] | None,
    candidate: dict[str, Any] | None,
) -> None:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    (artifact_dir / "oracle.c").write_text(oracle_code, encoding="utf-8")
    (artifact_dir / "candidate.c").write_text(candidate_code, encoding="utf-8")
    (artifact_dir / "result.json").write_text(
        dump_json_pretty(
            {
                "oracle": oracle,
                "candidate": candidate,
            }
        ),
        encoding="utf-8",
    )


def decomp_debug_command_for_row(row: dict[str, Any], fission_bin: Path, output_dir: Path) -> dict[str, Any] | None:
    address = row.get("address")
    binary_path = row.get("binary_path")
    if not address or not binary_path:
        return None
    bundle_path = debug_bundle_path_for_row(output_dir, row)
    cmd = [
        fission_bin,
        "decomp",
        resolve_path(str(binary_path)),
        "--addr",
        str(address),
        "--json",
        "--no-header",
        "--no-warnings",
        "--debug-decomp",
        "--debug-decomp-bundle",
        bundle_path,
    ]
    return {
        "debug_decomp_bundle_path": rel(bundle_path),
        "debug_decomp_command": shell_command(cmd),
        "disasm_function_command": shell_command(
            [
                fission_bin,
                "disasm",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--function",
                "--json",
            ]
        ),
        "xrefs_function_command": shell_command(
            [
                fission_bin,
                "xrefs",
                resolve_path(str(binary_path)),
                "--function",
                str(address),
                "--json",
            ]
        ),
        "preview_candidate_command": None,
        "preview_candidate_note": "inventory preview-candidates is deprecated with native_decomp removal; use debug-decomp and function-facts",
        "function_facts_command": shell_command(
            [
                fission_bin,
                "inventory",
                "function-facts",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--output-jsonl",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.jsonl",
                "--summary-json",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.json",
            ]
        ),
    }


def attach_debug_repro_commands(rows: list[dict[str, Any]], fission_bin: Path, output_dir: Path) -> None:
    for row in rows:
        command = decomp_debug_command_for_row(row, fission_bin, output_dir)
        if command is not None:
            row.update(command)


def top_debug_commands(rows: list[dict[str, Any]], limit: int = 12) -> list[dict[str, Any]]:
    candidates = [
        row
        for row in rows
        if row.get("debug_decomp_command") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    candidates.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return [
        {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": row.get("address"),
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "behavior_artifact_dir": row.get("behavior", {}).get("artifact_dir"),
            "debug_decomp_bundle_path": row.get("debug_decomp_bundle_path"),
            "debug_decomp_command": row.get("debug_decomp_command"),
            "disasm_function_command": row.get("disasm_function_command"),
            "xrefs_function_command": row.get("xrefs_function_command"),
            "preview_candidate_command": row.get("preview_candidate_command"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "function_facts_command": row.get("function_facts_command"),
        }
        for row in candidates[:limit]
    ]


def run_command_capture(cmd: list[Any], timeout_sec: int) -> dict[str, Any]:
    started = time.perf_counter()
    try:
        res = subprocess.run(
            [str(part) for part in cmd],
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=False,
        )
        return {
            "command": shell_command(cmd),
            "returncode": res.returncode,
            "timed_out": False,
            "wall_sec": round(time.perf_counter() - started, 6),
            "stdout": res.stdout,
            "stderr": res.stderr,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "command": shell_command(cmd),
            "returncode": None,
            "timed_out": True,
            "wall_sec": round(time.perf_counter() - started, 6),
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
        }


def materialize_debug_triage_for_rows(
    selected: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
) -> list[dict[str, Any]]:
    triage_rows: list[dict[str, Any]] = []
    for row in selected:
        binary_path = resolve_path(str(row["binary_path"]))
        address = str(row["address"])
        decomp_bundle_path = debug_bundle_path_for_row(output_dir, row)
        decomp_capture_path = debug_triage_path_for_row(output_dir, row, "debug_decomp", "command.json")
        disasm_capture_path = debug_triage_path_for_row(output_dir, row, "disasm", "command.json")
        xrefs_capture_path = debug_triage_path_for_row(output_dir, row, "xrefs", "command.json")
        facts_jsonl_path = debug_triage_path_for_row(output_dir, row, "function_facts", "jsonl")
        facts_summary_path = debug_triage_path_for_row(output_dir, row, "function_facts", "summary.json")
        facts_capture_path = debug_triage_path_for_row(output_dir, row, "function_facts", "command.json")
        decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        decomp_capture_path.parent.mkdir(parents=True, exist_ok=True)
        disasm_capture_path.parent.mkdir(parents=True, exist_ok=True)
        xrefs_capture_path.parent.mkdir(parents=True, exist_ok=True)
        facts_jsonl_path.parent.mkdir(parents=True, exist_ok=True)

        decomp_capture = run_command_capture(
            [
                fission_bin,
                "decomp",
                binary_path,
                "--addr",
                address,
                "--json",
                "--no-header",
                "--no-warnings",
                "--debug-decomp",
                "--debug-decomp-bundle",
                decomp_bundle_path,
            ],
            timeout_sec,
        )
        decomp_capture_path.write_text(dump_json_pretty(decomp_capture), encoding="utf-8")

        disasm_capture = run_command_capture(
            [
                fission_bin,
                "disasm",
                binary_path,
                "--addr",
                address,
                "--function",
                "--json",
            ],
            timeout_sec,
        )
        disasm_capture_path.write_text(dump_json_pretty(disasm_capture), encoding="utf-8")

        xrefs_capture = run_command_capture(
            [
                fission_bin,
                "xrefs",
                binary_path,
                "--function",
                address,
                "--json",
            ],
            timeout_sec,
        )
        xrefs_capture_path.write_text(dump_json_pretty(xrefs_capture), encoding="utf-8")

        facts = run_command_capture(
            [
                fission_bin,
                "inventory",
                "function-facts",
                binary_path,
                "--addr",
                address,
                "--output-jsonl",
                facts_jsonl_path,
                "--summary-json",
                facts_summary_path,
            ],
            timeout_sec,
        )
        facts_capture_path.write_text(dump_json_pretty(facts), encoding="utf-8")

        triage = {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": address,
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "baseline_regression": row.get("baseline_regression"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "debug_decomp_capture_path": rel(decomp_capture_path),
            "debug_decomp_bundle_path": rel(decomp_bundle_path),
            "debug_decomp_returncode": decomp_capture.get("returncode"),
            "disasm_capture_path": rel(disasm_capture_path),
            "disasm_returncode": disasm_capture.get("returncode"),
            "xrefs_capture_path": rel(xrefs_capture_path),
            "xrefs_returncode": xrefs_capture.get("returncode"),
            "function_facts_capture_path": rel(facts_capture_path),
            "function_facts_jsonl_path": rel(facts_jsonl_path),
            "function_facts_summary_path": rel(facts_summary_path),
            "function_facts_returncode": facts.get("returncode"),
        }
        row["debug_decomp_bundle_path"] = rel(decomp_bundle_path)
        row["debug_triage"] = triage
        triage_rows.append(triage)
    return triage_rows


def materialize_debug_triage(
    rows: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    selected = [
        row
        for row in rows
        if row.get("address") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    selected.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return materialize_debug_triage_for_rows(selected[: max(0, limit)], fission_bin, output_dir, timeout_sec)


def materialize_regression_debug_triage(
    rows: list[dict[str, Any]],
    comparison: dict[str, Any],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    rows_by_key = {row_key(row): row for row in rows if row.get("address")}
    selected: list[dict[str, Any]] = []
    seen: set[str] = set()
    for delta in comparison.get("top_regressions") or []:
        key = str(delta.get("key") or "")
        if not key or key in seen:
            continue
        row = rows_by_key.get(key)
        if row is None:
            continue
        row["baseline_regression"] = {
            "baseline_score_percent": delta.get("baseline_score_percent"),
            "current_score_percent": delta.get("current_score_percent"),
            "delta_percent": delta.get("delta_percent"),
            "baseline_behavior": delta.get("baseline_behavior"),
            "current_behavior": delta.get("current_behavior"),
        }
        selected.append(row)
        seen.add(key)
        if len(selected) >= max(0, limit):
            break
    return materialize_debug_triage_for_rows(selected, fission_bin, output_dir, timeout_sec)


def code_fingerprint(code: str, func: SourceFunction | None = None) -> Counter[str]:
    stripped = strip_comments(code)
    counter: Counter[str] = Counter()
    for word in WORD_BOUNDARY_RE.findall(stripped):
        lowered = word.lower()
        if lowered in CONTROL_WORDS:
            counter[f"ctrl:{lowered}"] += 1
    for op in ["<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "->", "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "="]:
        counter[f"op:{op}"] += stripped.count(op)
    for const in CONST_RE.findall(stripped):
        counter[f"const:{const.lower()}"] += 1
    for call in CALL_RE.findall(stripped):
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


def percent(value: float) -> float:
    return round(value * 100.0, 3)


def default_behavior_cases(param_count: int) -> list[tuple[int, ...]]:
    if param_count == 0:
        return [()]
    if param_count == 1:
        return [(0,), (1,), (2,), (5,), (10,), (-1,)]
    if param_count == 2:
        return [(0, 0), (1, 2), (5, 10), (-3, 7), (42, -1)]
    if param_count == 3:
        return [(1, 2, 3), (0, 5, -1), (7, 3, 2)]
    return []


def default_behavior_cases_for_kinds(param_kinds: list[str]) -> list[tuple[int, ...]]:
    unsigned_positions = {idx for idx, kind in enumerate(param_kinds) if kind == "uint"}
    cases = default_behavior_cases(len(param_kinds))
    if not unsigned_positions:
        return cases
    return [
        case
        for case in cases
        if all(case[idx] >= 0 for idx in unsigned_positions)
    ]


def explicit_behavior_cases(entry: BenchmarkEntry, func: SourceFunction) -> list[dict[str, Any]] | None:
    if not entry.behavior_cases:
        return None
    cases = entry.behavior_cases.get(func.name)
    if cases is None:
        return None
    return cases


def behavior_supported(
    entry: BenchmarkEntry, func: SourceFunction, explicit_cases: list[dict[str, Any]] | None
) -> tuple[bool, str | None]:
    language = entry.language
    if language != "c":
        return False, "dynamic harness currently supports C source functions only"
    if func.name == "main":
        return False, "main is not called as a unit function"
    if explicit_cases is not None:
        if func.return_kind not in {"int", "void"}:
            return False, f"unsupported return kind: {func.return_kind}"
        unsupported = [kind for kind in func.param_kinds if kind not in {"int", "uint", "int_ptr"}]
        if unsupported:
            return False, f"unsupported parameter kinds: {func.param_kinds}"
        valid, reason = validate_explicit_behavior_cases(func, explicit_cases)
        if not valid:
            return False, reason
        return True, None
    if func.return_kind != "int":
        return False, f"unsupported return kind: {func.return_kind}"
    if any(kind not in {"int", "uint"} for kind in func.param_kinds):
        return False, f"unsupported parameter kinds: {func.param_kinds}"
    if not default_behavior_cases_for_kinds(func.param_kinds):
        return False, "unsupported arity"
    return True, None


def validate_explicit_behavior_cases(
    func: SourceFunction, cases: list[dict[str, Any]]
) -> tuple[bool, str | None]:
    if not cases:
        return False, "empty explicit behavior case list"
    for case_index, case in enumerate(cases):
        args = case.get("args")
        if not isinstance(args, list):
            return False, f"case {case_index} missing args list"
        if len(args) != len(func.param_kinds):
            return False, f"case {case_index} arity mismatch"
        for arg_index, (arg, kind) in enumerate(zip(args, func.param_kinds)):
            if kind in {"int", "uint"} and not isinstance(arg, int):
                return False, f"case {case_index} arg {arg_index} must be int"
            if kind == "int_ptr":
                if not isinstance(arg, dict) or "int_array" not in arg:
                    return False, f"case {case_index} arg {arg_index} must be int_array"
                values = arg["int_array"]
                if not isinstance(values, list) or not all(isinstance(v, int) for v in values):
                    return False, f"case {case_index} arg {arg_index} has invalid int_array"
        globals_to_observe = case.get("globals", [])
        if globals_to_observe is None:
            globals_to_observe = []
        if not isinstance(globals_to_observe, list):
            return False, f"case {case_index} globals must be a list"
        for global_index, global_spec in enumerate(globals_to_observe):
            if not isinstance(global_spec, dict):
                return False, f"case {case_index} global {global_index} must be an object"
            name = global_spec.get("name")
            if not isinstance(name, str) or not WORD_BOUNDARY_RE.fullmatch(name):
                return False, f"case {case_index} global {global_index} has invalid name"
            ctype = global_spec.get("ctype", "unsigned int")
            if not isinstance(ctype, str) or not CTYPE_RE.fullmatch(ctype.strip()):
                return False, f"case {case_index} global {global_index} has invalid ctype"
            reset = global_spec.get("reset", 0)
            if not isinstance(reset, int):
                return False, f"case {case_index} global {global_index} reset must be int"
    return True, None


def behavior_cases_for(
    entry: BenchmarkEntry, func: SourceFunction
) -> list[tuple[int, ...]] | list[dict[str, Any]]:
    explicit_cases = explicit_behavior_cases(entry, func)
    if explicit_cases is not None:
        return explicit_cases
    return default_behavior_cases_for_kinds(func.param_kinds)


def source_harness(
    source_path: Path, func: SourceFunction, cases: list[tuple[int, ...]] | list[dict[str, Any]]
) -> str:
    calls = "\n".join(render_case_call(func, case, index) for index, case in enumerate(cases))
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


def candidate_harness(
    candidate_code: str, func: SourceFunction, cases: list[tuple[int, ...]] | list[dict[str, Any]]
) -> str:
    calls = "\n".join(render_case_call(func, case, index) for index, case in enumerate(cases))
    observed_globals = collect_observed_globals(cases)
    globals_decl = "\n".join(render_candidate_global_decl(spec) for spec in observed_globals)
    candidate_code = remove_duplicate_candidate_global_decls(candidate_code, observed_globals)
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
static inline bool __fission_carry32(uint32 a, uint32 b) {{ return (uint32)(a + b) < a; }}
static inline bool __fission_carry64(uint64 a, uint64 b) {{ return (uint64)(a + b) < a; }}
static inline bool __fission_scarry32(uint32 a, uint32 b) {{
    int32 sa = (int32)a, sb = (int32)b, sr = (int32)(a + b);
    return ((sa ^ sr) & (sb ^ sr)) < 0;
}}
static inline bool __fission_scarry64(uint64 a, uint64 b) {{
    int64 sa = (int64)a, sb = (int64)b, sr = (int64)(a + b);
    return ((sa ^ sr) & (sb ^ sr)) < 0;
}}
static inline bool __fission_sborrow32(uint32 a, uint32 b) {{
    int32 sa = (int32)a, sb = (int32)b, sr = (int32)(a - b);
    return ((sa ^ sb) & (sa ^ sr)) < 0;
}}
static inline bool __fission_sborrow64(uint64 a, uint64 b) {{
    int64 sa = (int64)a, sb = (int64)b, sr = (int64)(a - b);
    return ((sa ^ sb) & (sa ^ sr)) < 0;
}}
#define __carry(a, b) (sizeof(a) <= 4 ? __fission_carry32((uint32)(a), (uint32)(b)) : __fission_carry64((uint64)(a), (uint64)(b)))
#define __scarry(a, b) (sizeof(a) <= 4 ? __fission_scarry32((uint32)(a), (uint32)(b)) : __fission_scarry64((uint64)(a), (uint64)(b)))
#define __sborrow(a, b) (sizeof(a) <= 4 ? __fission_sborrow32((uint32)(a), (uint32)(b)) : __fission_sborrow64((uint64)(a), (uint64)(b)))
{globals_decl}
{candidate_code}
int main(void) {{
{calls}
    return 0;
}}
"""


def render_case_call(func: SourceFunction, case: tuple[int, ...] | dict[str, Any], index: int) -> str:
    if isinstance(case, dict):
        return render_explicit_case_call(func, case, index)
    args = ", ".join(str(v) for v in case)
    return f'    printf("%lld\\n", (long long){func.name}({args}));\n    fflush(stdout);'


def c_int_array(values: list[int]) -> str:
    return ", ".join(str(v) for v in values) or "0"


def collect_observed_globals(cases: list[tuple[int, ...]] | list[dict[str, Any]]) -> list[dict[str, Any]]:
    observed: dict[str, dict[str, Any]] = {}
    for case in cases:
        if not isinstance(case, dict):
            continue
        for spec in case.get("globals") or []:
            name = spec["name"]
            observed.setdefault(
                name,
                {
                    "name": name,
                    "ctype": spec.get("ctype", "unsigned int"),
                    "reset": spec.get("reset", 0),
                },
            )
    return [observed[name] for name in sorted(observed)]


def render_candidate_global_decl(spec: dict[str, Any]) -> str:
    return f"volatile {spec.get('ctype', 'unsigned int')} {spec['name']} = {int(spec.get('reset', 0))};"


def remove_duplicate_candidate_global_decls(candidate_code: str, observed_globals: list[dict[str, Any]]) -> str:
    names = {
        spec["name"]
        for spec in observed_globals
        if isinstance(spec.get("name"), str) and WORD_BOUNDARY_RE.fullmatch(spec["name"])
    }
    if not names:
        return candidate_code

    def is_duplicate_decl(line: str) -> bool:
        stripped = line.strip()
        if not stripped.endswith(";"):
            return False
        if "(" in stripped:
            return False
        for name in names:
            if re.fullmatch(
                rf"(?:extern\s+)?(?:volatile\s+)?[A-Za-z_][A-Za-z0-9_\s\*]*\b{re.escape(name)}\s*(?:=\s*[^;]+)?;",
                stripped,
            ):
                return True
        return False

    return "\n".join(line for line in candidate_code.splitlines() if not is_duplicate_decl(line))


def render_explicit_case_call(func: SourceFunction, case: dict[str, Any], index: int) -> str:
    args = case["args"]
    setup: list[str] = []
    call_args: list[str] = []
    pointer_arrays: list[tuple[int, str, int]] = []
    for arg_index, (arg, kind) in enumerate(zip(args, func.param_kinds)):
        if kind in {"int", "uint"}:
            call_args.append(str(arg))
            continue
        if kind == "int_ptr":
            values = arg["int_array"]
            name = f"case_{index}_arg_{arg_index}"
            setup.append(f"    int {name}[] = {{{c_int_array(values)}}};")
            call_args.append(name)
            pointer_arrays.append((arg_index, name, len(values)))
            continue
        raise AssertionError(f"unsupported explicit behavior kind: {kind}")

    joined_args = ", ".join(call_args)
    globals_to_observe = case.get("globals") or []
    lines = setup
    for spec in globals_to_observe:
        lines.append(f"    {spec['name']} = {int(spec.get('reset', 0))};")
    if func.return_kind == "void":
        lines.append(f"    {func.name}({joined_args});")
        lines.append('    printf("ret=void");')
    else:
        lines.append(f"    long long case_{index}_ret = (long long){func.name}({joined_args});")
        lines.append(f'    printf("ret=%lld", case_{index}_ret);')
    for arg_index, array_name, length in pointer_arrays:
        lines.append(f'    printf(" arg{arg_index}=");')
        lines.append(f"    for (int i = 0; i < {length}; ++i) {{")
        lines.append(f'        printf("%s%d", i ? "," : "", {array_name}[i]);')
        lines.append("    }")
    for spec in globals_to_observe:
        lines.append(f'    printf(" {spec["name"]}=%lld", (long long){spec["name"]});')
    lines.append('    printf("\\n");')
    lines.append("    fflush(stdout);")
    return "\n".join(lines)


def serialize_behavior_cases(cases: list[tuple[int, ...]] | list[dict[str, Any]]) -> list[Any]:
    serialized: list[Any] = []
    for case in cases:
        if isinstance(case, dict):
            serialized.append(case)
        else:
            serialized.append(list(case))
    return serialized


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
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode("utf-8", errors="replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode("utf-8", errors="replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        return {
            "status": "run_timeout",
            "partial_stdout": stdout[-4000:],
            "partial_stderr": stderr[-4000:],
        }

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
    output_dir: Path | None = None,
    address: str | None = None,
) -> dict[str, Any]:
    explicit_cases = explicit_behavior_cases(entry, func)
    supported, reason = behavior_supported(entry, func, explicit_cases)
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

    cases = behavior_cases_for(entry, func)
    oracle_code = source_harness(entry.source_path, func, cases)
    candidate_code = candidate_harness(decomp_code, func, cases)
    artifact_dir = (
        behavior_artifact_dir_for_row(output_dir, entry, func, address)
        if output_dir is not None
        else None
    )

    def maybe_attach_artifacts(result: dict[str, Any], oracle: dict[str, Any] | None, candidate: dict[str, Any] | None) -> dict[str, Any]:
        if artifact_dir is None or result.get("status") == "pass":
            return result
        write_behavior_artifacts(artifact_dir, oracle_code, candidate_code, oracle, candidate)
        result["artifact_dir"] = rel(artifact_dir)
        result["oracle_source_path"] = rel(artifact_dir / "oracle.c")
        result["candidate_source_path"] = rel(artifact_dir / "candidate.c")
        result["result_path"] = rel(artifact_dir / "result.json")
        return result

    with tempfile.TemporaryDirectory(prefix="source-semantic-") as tmp:
        tmp_path = Path(tmp)
        oracle = compile_and_run_c(oracle_code, tmp_path, "oracle", timeout_sec)
        if oracle.get("status") != "ok":
            return maybe_attach_artifacts(
                {"status": f"oracle_{oracle.get('status')}", "score": 0.0, "detail": oracle.get("detail")},
                oracle,
                None,
            )
        candidate = compile_and_run_c(candidate_code, tmp_path, "candidate", timeout_sec)
        if candidate.get("status") != "ok":
            return maybe_attach_artifacts(
                {"status": f"candidate_{candidate.get('status')}", "score": 0.0, "detail": candidate.get("detail")},
                oracle,
                candidate,
            )
        oracle_lines = [line.strip() for line in oracle["stdout"].splitlines() if line.strip()]
        candidate_lines = [line.strip() for line in candidate["stdout"].splitlines() if line.strip()]
        passed = oracle_lines == candidate_lines
        return maybe_attach_artifacts({
            "status": "pass" if passed else "mismatch",
            "score": 1.0 if passed else 0.0,
            "cases": serialize_behavior_cases(cases),
            "oracle": oracle_lines,
            "candidate": candidate_lines,
        }, oracle, candidate)


def summarize(rows: list[dict[str, Any]], manifest_name: str, entries: list[BenchmarkEntry]) -> dict[str, Any]:
    total = len(rows)
    mapped = sum(1 for row in rows if row["mapping_status"] == "matched")
    decomp_ok = sum(1 for row in rows if row.get("decomp_success"))
    compile_ok = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    behavior_pass = sum(1 for row in rows if row.get("behavior", {}).get("status") == "pass")
    score_values = [float(row.get("semantic_score", 0.0) or 0.0) for row in rows]
    mapping_status_counts = Counter(row.get("mapping_status", "unknown") for row in rows)
    decomp_failure_counts = Counter(
        row.get("decomp_failure_kind", "unknown")
        for row in rows
        if not row.get("decomp_success")
    )
    behavior_status_counts = Counter(row.get("behavior", {}).get("status", "unknown") for row in rows)
    decomp_cache_status_counts = Counter(row.get("decomp_cache_status", "not_requested") for row in rows)
    debug_owner_bucket_counts: Counter[str] = Counter()
    debug_stage_status_counts: Counter[str] = Counter()
    debug_quality_evidence_totals: Counter[str] = Counter()
    debug_template_source_totals: Counter[str] = Counter()
    by_language: dict[str, dict[str, Any]] = {}
    by_tag: dict[str, dict[str, Any]] = {}
    by_entry: dict[str, dict[str, Any]] = {}

    def add_bucket(bucket: dict[str, Any], row: dict[str, Any]) -> None:
        bucket["row_count"] += 1
        bucket["mapped"] += int(row["mapping_status"] == "matched")
        bucket["decomp_success"] += int(bool(row.get("decomp_success")))
        bucket["behavior_pass"] += int(row.get("behavior", {}).get("status") == "pass")
        bucket["score_sum"] += float(row.get("semantic_score", 0.0) or 0.0)

    for row in rows:
        lang = row["language"]
        bucket = by_language.setdefault(
            lang,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(bucket, row)

        entry_bucket = by_entry.setdefault(
            row["entry_id"],
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(entry_bucket, row)

        debug_decomp = row.get("debug_decomp")
        if isinstance(debug_decomp, dict):
            debug_owner_bucket_counts.update(debug_decomp.get("owner_buckets") or [])
            stage_status = debug_decomp.get("stage_status")
            if isinstance(stage_status, dict):
                debug_stage_status_counts.update(
                    f"{stage}:{status}"
                    for stage, status in stage_status.items()
                    if status is not None
                )
            quality = debug_decomp.get("quality_evidence")
            if isinstance(quality, dict):
                for key, value in quality.items():
                    if isinstance(value, int | float):
                        debug_quality_evidence_totals[key] += value
            pipeline = debug_decomp.get("rust_sleigh_pipeline")
            template_sources = (
                pipeline.get("template_source_counts")
                if isinstance(pipeline, dict)
                and isinstance(pipeline.get("template_source_counts"), dict)
                else {}
            )
            for key, value in template_sources.items():
                if isinstance(value, int | float):
                    debug_template_source_totals[key] += value

        for tag in row.get("tags") or []:
            tag_bucket = by_tag.setdefault(
                tag,
                {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
            )
            add_bucket(tag_bucket, row)

    for bucket in list(by_language.values()) + list(by_tag.values()) + list(by_entry.values()):
        count = max(1, bucket["row_count"])
        avg_score = round(bucket.pop("score_sum") / count, 6)
        bucket["avg_semantic_score"] = avg_score
        bucket["avg_semantic_score_percent"] = percent(avg_score)
    host_statuses = Counter(
        row.get("behavior", {}).get("reason")
        for row in rows
        if row.get("behavior", {}).get("status") == "host_execution_unavailable"
    )
    weighted_semantic_similarity = round(sum(score_values) / total, 6) if total else 0.0
    return {
        "manifest": manifest_name,
        "entry_count": len(entries),
        "row_count": total,
        "function_mapping_rate": round(mapped / total, 6) if total else 0.0,
        "decomp_success_rate": round(decomp_ok / total, 6) if total else 0.0,
        "candidate_compile_rate": round(compile_ok / total, 6) if total else 0.0,
        "behavior_pass_rate": round(behavior_pass / total, 6) if total else 0.0,
        "weighted_semantic_similarity": weighted_semantic_similarity,
        "weighted_semantic_similarity_percent": percent(weighted_semantic_similarity),
        "perfect_row_count": sum(1 for score in score_values if score == 1.0),
        "supported_behavior_row_count": sum(
            1 for row in rows if row.get("behavior", {}).get("status") != "unsupported_signature"
        ),
        "mapping_status_counts": dict(sorted(mapping_status_counts.items())),
        "decomp_failure_counts": dict(sorted(decomp_failure_counts.items())),
        "behavior_status_counts": dict(sorted(behavior_status_counts.items())),
        "decomp_cache_status_counts": dict(sorted(decomp_cache_status_counts.items())),
        "debug_owner_bucket_counts": dict(sorted(debug_owner_bucket_counts.items())),
        "debug_stage_status_counts": dict(sorted(debug_stage_status_counts.items())),
        "debug_quality_evidence_totals": dict(sorted(debug_quality_evidence_totals.items())),
        "debug_template_source_totals": dict(sorted(debug_template_source_totals.items())),
        "host_execution_unavailable_count": sum(host_statuses.values()),
        "host_execution_unavailable_reasons": dict(host_statuses),
        "by_language": by_language,
        "by_tag": by_tag,
        "by_entry": by_entry,
    }


def row_key(row: dict[str, Any]) -> str:
    return "::".join(
        [
            str(row.get("entry_id") or ""),
            str(row.get("source_path") or ""),
            str(row.get("function_name") or ""),
        ]
    )


def sleigh_template_source_gate(summary: dict[str, Any], required_source: str) -> dict[str, Any]:
    template_totals = summary.get("debug_template_source_totals")
    if not isinstance(template_totals, dict):
        template_totals = {}
    stage_counts = summary.get("debug_stage_status_counts")
    if not isinstance(stage_counts, dict):
        stage_counts = {}

    failures: list[str] = []
    row_count = int(summary.get("row_count", 0) or 0)
    raw_pcode_ok = int(stage_counts.get("raw_pcode:ok", 0) or 0)
    total_templates = sum(
        int(value) for value in template_totals.values() if isinstance(value, int | float)
    )
    unexpected_sources = {
        source: int(value)
        for source, value in template_totals.items()
        if source != required_source and isinstance(value, int | float) and int(value) != 0
    }

    if row_count > 0 and total_templates == 0:
        failures.append(
            "SLEIGH template source gate requires debug_template_source_totals; run with --include-debug-decomp"
        )
    if unexpected_sources:
        failures.append(
            f"SLEIGH template sources must be only {required_source!r} "
            f"(unexpected {unexpected_sources})"
        )

    return {
        "required_source": required_source,
        "status": "passed" if not failures else "failed",
        "failures": failures,
        "template_source_totals": dict(sorted(template_totals.items())),
        "row_count": row_count,
        "raw_pcode_ok_rows": raw_pcode_ok,
    }


def load_baseline_artifacts(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], Path]:
    summary_path = path
    if path.is_dir():
        summary_path = path / "source_semantic_summary.json"
    summary = load_json(summary_path)
    rows_path = summary_path.parent / "source_semantic_rows.json"
    rows = load_json(rows_path) if rows_path.exists() else []
    if not isinstance(rows, list):
        rows = []
    return summary, rows, summary_path


def find_latest_baseline_dir(
    output_dir: Path,
    manifest_name: str,
    current_row_keys: set[str],
) -> Path | None:
    root = DEFAULT_ARTIFACT_ROOT
    if not root.exists():
        return None
    output_resolved = output_dir.resolve()
    candidates: list[tuple[float, Path]] = []
    for summary_path in root.rglob("source_semantic_summary.json"):
        try:
            parent_resolved = summary_path.parent.resolve()
        except OSError:
            continue
        if parent_resolved == output_resolved:
            continue
        try:
            summary = load_json(summary_path)
        except Exception:
            continue
        if summary.get("manifest") != manifest_name:
            continue
        rows_path = summary_path.parent / "source_semantic_rows.json"
        baseline_keys: set[str] = set()
        if rows_path.exists():
            try:
                raw_rows = load_json(rows_path)
                if isinstance(raw_rows, list):
                    baseline_keys = {row_key(row) for row in raw_rows if isinstance(row, dict)}
            except Exception:
                baseline_keys = set()
        try:
            mtime = summary_path.stat().st_mtime
        except OSError:
            continue
        if current_row_keys and baseline_keys != current_row_keys:
            continue
        if not current_row_keys and summary.get("row_count") != 0:
            continue
        candidates.append((mtime, summary_path.parent))
    if not candidates:
        return None
    return max(candidates, key=lambda item: item[0])[1]


def metric_delta(current: dict[str, Any], baseline: dict[str, Any], key: str) -> dict[str, Any]:
    current_value = current.get(key)
    baseline_value = baseline.get(key)
    if key.endswith("_percent"):
        raw_key = key.removesuffix("_percent")
        if not isinstance(current_value, (int, float)) and isinstance(current.get(raw_key), (int, float)):
            current_value = percent(float(current[raw_key]))
        if not isinstance(baseline_value, (int, float)) and isinstance(baseline.get(raw_key), (int, float)):
            baseline_value = percent(float(baseline[raw_key]))
    if not isinstance(current_value, (int, float)) or not isinstance(baseline_value, (int, float)):
        return {"current": current_value, "baseline": baseline_value, "delta": None}
    return {
        "current": current_value,
        "baseline": baseline_value,
        "delta": round(float(current_value) - float(baseline_value), 6),
    }


def compare_to_baseline(
    summary: dict[str, Any],
    rows: list[dict[str, Any]],
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    baseline_path: Path,
) -> dict[str, Any]:
    current_by_key = {row_key(row): row for row in rows}
    baseline_by_key = {row_key(row): row for row in baseline_rows}
    shared_keys = sorted(set(current_by_key) & set(baseline_by_key))
    new_keys = sorted(set(current_by_key) - set(baseline_by_key))
    missing_keys = sorted(set(baseline_by_key) - set(current_by_key))

    row_deltas: list[dict[str, Any]] = []
    improved = 0
    regressed = 0
    unchanged = 0
    behavior_improved = 0
    behavior_regressed = 0
    for key in shared_keys:
        current = current_by_key[key]
        baseline = baseline_by_key[key]
        current_score = float(current.get("semantic_score", 0.0) or 0.0)
        baseline_score = float(baseline.get("semantic_score", 0.0) or 0.0)
        delta = round(current_score - baseline_score, 6)
        if delta > 0:
            improved += 1
        elif delta < 0:
            regressed += 1
        else:
            unchanged += 1

        current_behavior = current.get("behavior", {}).get("status")
        baseline_behavior = baseline.get("behavior", {}).get("status")
        if current_behavior == "pass" and baseline_behavior != "pass":
            behavior_improved += 1
        elif current_behavior != "pass" and baseline_behavior == "pass":
            behavior_regressed += 1

        if delta != 0 or current_behavior != baseline_behavior:
            row_deltas.append(
                {
                    "key": key,
                    "entry_id": current.get("entry_id"),
                    "function_name": current.get("function_name"),
                    "current_score": current_score,
                    "baseline_score": baseline_score,
                    "delta": delta,
                    "current_score_percent": percent(current_score),
                    "baseline_score_percent": percent(baseline_score),
                    "delta_percent": percent(delta),
                    "current_behavior": current_behavior,
                    "baseline_behavior": baseline_behavior,
                    "current_mapping_status": current.get("mapping_status"),
                    "baseline_mapping_status": baseline.get("mapping_status"),
                    "current_decomp_failure_kind": current.get("decomp_failure_kind"),
                    "baseline_decomp_failure_kind": baseline.get("decomp_failure_kind"),
                }
            )

    row_deltas.sort(key=lambda row: (abs(float(row["delta"])), row["function_name"] or ""), reverse=True)
    top_improvements = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) > 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
        reverse=True,
    )[:10]
    top_regressions = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) < 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
    )[:10]
    metric_keys = [
        "weighted_semantic_similarity",
        "weighted_semantic_similarity_percent",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "perfect_row_count",
        "supported_behavior_row_count",
        "row_count",
    ]
    return {
        "baseline_summary_path": rel(baseline_path),
        "shared_row_count": len(shared_keys),
        "new_row_count": len(new_keys),
        "missing_row_count": len(missing_keys),
        "improved_row_count": improved,
        "regressed_row_count": regressed,
        "unchanged_row_count": unchanged,
        "behavior_improved_row_count": behavior_improved,
        "behavior_regressed_row_count": behavior_regressed,
        "metric_deltas": {key: metric_delta(summary, baseline_summary, key) for key in metric_keys},
        "top_row_deltas": row_deltas[:20],
        "top_improvements": top_improvements,
        "top_regressions": top_regressions,
        "new_rows": [current_by_key[key].get("function_name") for key in new_keys[:20]],
        "missing_rows": [baseline_by_key[key].get("function_name") for key in missing_keys[:20]],
    }


def comparison_outcome(comparison: dict[str, Any]) -> dict[str, Any]:
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
    )
    behavior_improved = int(comparison.get("behavior_improved_row_count") or 0)
    behavior_regressed = int(comparison.get("behavior_regressed_row_count") or 0)
    improved = int(comparison.get("improved_row_count") or 0)
    regressed = int(comparison.get("regressed_row_count") or 0)
    shape_changed = bool(comparison.get("new_row_count") or comparison.get("missing_row_count"))
    if shape_changed:
        direction = "mixed"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta > 0 and behavior_regressed == 0:
        direction = "improved"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta < 0 and behavior_improved == 0:
        direction = "regressed"
    elif improved == 0 and regressed == 0 and behavior_improved == 0 and behavior_regressed == 0:
        direction = "unchanged"
    else:
        direction = "mixed"
    delta_text = "n/a" if not isinstance(weighted_delta, (int, float)) else f"{weighted_delta:+.3f}%"
    return {
        "direction": direction,
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "headline": (
            f"{direction}: weighted semantic similarity {delta_text}, "
            f"rows +{improved}/-{regressed}, behavior +{behavior_improved}/-{behavior_regressed}"
        ),
    }


def improvement_summary(comparison: dict[str, Any]) -> dict[str, Any]:
    metric_deltas = comparison.get("metric_deltas") if isinstance(comparison.get("metric_deltas"), dict) else {}

    def delta_for(key: str) -> float | None:
        metric = metric_deltas.get(key)
        if not isinstance(metric, dict):
            return None
        delta = metric.get("delta")
        return float(delta) if isinstance(delta, int | float) else None

    improved_metrics: list[dict[str, Any]] = []
    regressed_metrics: list[dict[str, Any]] = []
    for key in [
        "weighted_semantic_similarity_percent",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "perfect_row_count",
        "supported_behavior_row_count",
    ]:
        delta = delta_for(key)
        if delta is None or delta == 0:
            continue
        metric = {
            "metric": key,
            "delta": delta,
            "current": metric_deltas.get(key, {}).get("current"),
            "baseline": metric_deltas.get(key, {}).get("baseline"),
        }
        if delta > 0:
            improved_metrics.append(metric)
        else:
            regressed_metrics.append(metric)

    return {
        "headline": comparison_outcome(comparison)["headline"],
        "improved_metric_count": len(improved_metrics),
        "regressed_metric_count": len(regressed_metrics),
        "improved_metrics": improved_metrics,
        "regressed_metrics": regressed_metrics,
        "top_improved_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_improvements") or [])[:10]
        ],
        "top_regressed_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_regressions") or [])[:10]
        ],
    }


def snapshot_baseline_artifacts(
    output_dir: Path,
    baseline_summary_path: Path,
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    comparison: dict[str, Any],
) -> dict[str, Any]:
    snapshot_dir = output_dir / "baseline_snapshot"
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    summary_snapshot_path = snapshot_dir / "source_semantic_summary.json"
    rows_snapshot_path = snapshot_dir / "source_semantic_rows.json"
    comparison_snapshot_path = snapshot_dir / "source_semantic_comparison.json"
    manifest_path = snapshot_dir / "snapshot.json"
    summary_snapshot_path.write_text(dump_json_pretty(baseline_summary), encoding="utf-8")
    rows_snapshot_path.write_text(dump_json_pretty(baseline_rows), encoding="utf-8")
    comparison_snapshot_path.write_text(dump_json_pretty(comparison), encoding="utf-8")
    manifest = {
        "format": "source-semantic-baseline-snapshot-v1",
        "created_at_utc": utc_isoformat(utc_now()),
        "baseline_summary_path": rel(baseline_summary_path),
        "baseline_artifact_dir": rel(baseline_summary_path.parent),
        "summary_snapshot_path": rel(summary_snapshot_path),
        "rows_snapshot_path": rel(rows_snapshot_path),
        "comparison_snapshot_path": rel(comparison_snapshot_path),
    }
    manifest_path.write_text(dump_json_pretty(manifest), encoding="utf-8")
    manifest["snapshot_manifest_path"] = rel(manifest_path)
    return manifest


def append_history_record(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    comparison = summary.get("comparison") if isinstance(summary.get("comparison"), dict) else {}
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
        if isinstance(comparison, dict)
        else None
    )
    record = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "manifest": summary.get("manifest"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "comparison_outcome": summary.get("comparison_outcome"),
        "behavior_pass_rate": summary.get("behavior_pass_rate"),
        "candidate_compile_rate": summary.get("candidate_compile_rate"),
        "decomp_success_rate": summary.get("decomp_success_rate"),
        "baseline_summary_path": comparison.get("baseline_summary_path") if isinstance(comparison, dict) else None,
        "decomp_cache_hit_count": summary.get("decomp_cache_hit_count"),
        "decomp_cache_miss_count": summary.get("decomp_cache_miss_count"),
        "list_cache_hit_count": summary.get("list_cache_hit_count"),
        "list_cache_miss_count": summary.get("list_cache_miss_count"),
        "wall_sec": summary.get("wall_sec"),
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(dump_json_line(record))


def update_latest_index(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        index = load_json(path) if path.exists() else {}
    except Exception:
        index = {}
    if not isinstance(index, dict):
        index = {}
    manifest = str(summary.get("manifest") or "unknown")
    index[manifest] = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "summary_path": str(Path(str(summary.get("artifact_dir") or "")) / "source_semantic_summary.json"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "comparison_outcome": summary.get("comparison_outcome"),
        "decomp_cache_file": summary.get("decomp_cache_file"),
        "list_cache_file": summary.get("list_cache_file"),
        "history_file": summary.get("history_file"),
    }
    path.write_text(dump_json_pretty(index), encoding="utf-8")


def load_history_records(path: Path, manifest_name: str, limit: int = 12) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: list[dict[str, Any]] = []
    try:
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(record, dict) and record.get("manifest") == manifest_name:
                    records.append(record)
    except OSError:
        return []
    return records[-limit:]


def history_snapshot(path: Path, summary: dict[str, Any]) -> dict[str, Any] | None:
    records = load_history_records(path, str(summary.get("manifest") or ""))
    if not records:
        return None
    same_shape_records = [record for record in records if record.get("row_count") == summary.get("row_count")]
    comparison_record = same_shape_records[-1] if same_shape_records else records[-1]
    latest_record = records[-1]
    current_similarity = summary.get("weighted_semantic_similarity_percent")
    comparison_similarity = comparison_record.get("weighted_semantic_similarity_percent")
    latest_similarity = latest_record.get("weighted_semantic_similarity_percent")
    comparable_shape = comparison_record.get("row_count") == summary.get("row_count")
    comparison_delta = (
        round(float(current_similarity) - float(comparison_similarity), 6)
        if comparable_shape
        and isinstance(current_similarity, (int, float))
        and isinstance(comparison_similarity, (int, float))
        else None
    )
    latest_delta = (
        round(float(current_similarity) - float(latest_similarity), 6)
        if latest_record.get("row_count") == summary.get("row_count")
        and isinstance(current_similarity, (int, float))
        and isinstance(latest_similarity, (int, float))
        else None
    )
    return {
        "history_file": rel(path),
        "previous_run_count": len(records),
        "latest_previous_run": latest_record,
        "comparison_previous_run": comparison_record,
        "comparison_shape_matches": comparable_shape,
        "latest_shape_matches": latest_record.get("row_count") == summary.get("row_count"),
        "weighted_semantic_similarity_percent_delta_vs_comparison": comparison_delta,
        "weighted_semantic_similarity_percent_delta_vs_latest": latest_delta,
        "recent_runs": records,
    }


def render_markdown(summary: dict[str, Any], rows: list[dict[str, Any]]) -> str:
    lines = [
        f"# Source Semantic Benchmark: {summary['manifest']}",
        "",
        f"- Run ID: `{summary.get('run_id', 'unknown')}`",
        f"- Artifact dir: `{summary.get('artifact_dir', 'unknown')}`",
        f"- Entries: {summary['entry_count']}",
        f"- Rows: {summary['row_count']}",
        f"- Function mapping rate: {summary['function_mapping_rate']:.3f}",
        f"- Decompile success rate: {summary['decomp_success_rate']:.3f}",
        f"- Candidate compile rate: {summary['candidate_compile_rate']:.3f}",
        f"- Behavior pass rate: {summary['behavior_pass_rate']:.3f}",
        f"- Weighted semantic similarity: {summary['weighted_semantic_similarity_percent']:.3f}%",
        f"- Perfect rows: {summary['perfect_row_count']}",
        f"- Supported behavior rows: {summary['supported_behavior_row_count']}",
        f"- Host execution unavailable rows: {summary['host_execution_unavailable_count']}",
    ]
    if "wall_sec" in summary:
        lines.append(f"- Wall time: {summary['wall_sec']:.3f}s")
    if summary.get("decomp_cache_file"):
        lines.append(f"- Decomp cache: `{summary['decomp_cache_file']}`")
        lines.append(
            f"- Decomp cache hits/misses: {summary.get('decomp_cache_hit_count', 0)}/"
            f"{summary.get('decomp_cache_miss_count', 0)}"
        )
    if summary.get("list_cache_file"):
        lines.append(f"- List cache: `{summary['list_cache_file']}`")
        lines.append(
            f"- List cache hits/misses: {summary.get('list_cache_hit_count', 0)}/"
            f"{summary.get('list_cache_miss_count', 0)}"
        )
    if summary.get("history_file"):
        lines.append(f"- History: `{summary['history_file']}`")
    if summary.get("latest_index_file"):
        lines.append(f"- Latest index: `{summary['latest_index_file']}`")
    history = summary.get("history")
    if isinstance(history, dict):
        comparison_record = (
            history.get("comparison_previous_run")
            if isinstance(history.get("comparison_previous_run"), dict)
            else {}
        )
        latest_record = (
            history.get("latest_previous_run")
            if isinstance(history.get("latest_previous_run"), dict)
            else {}
        )
        comparison_delta = history.get("weighted_semantic_similarity_percent_delta_vs_comparison")
        latest_delta = history.get("weighted_semantic_similarity_percent_delta_vs_latest")
        comparison_delta_text = "n/a" if comparison_delta is None else f"{comparison_delta:+.3f}%"
        latest_delta_text = "n/a" if latest_delta is None else f"{latest_delta:+.3f}%"
        lines.append(
            f"- Latest comparable history delta: {comparison_delta_text} "
            f"(previous run `{comparison_record.get('run_id', 'unknown')}`)"
        )
        lines.append(
            f"- Latest history delta: {latest_delta_text} "
            f"(previous run `{latest_record.get('run_id', 'unknown')}`)"
        )
    if summary.get("baseline_snapshot"):
        snapshot = summary["baseline_snapshot"]
        lines.append(f"- Baseline snapshot: `{snapshot.get('snapshot_manifest_path')}`")
    improvement = summary.get("improvement_summary")
    if isinstance(improvement, dict):
        lines.extend(["", "## Improvement Summary", "", f"- {improvement.get('headline', 'n/a')}"])
        improved_metrics = improvement.get("improved_metrics") or []
        regressed_metrics = improvement.get("regressed_metrics") or []
        if improved_metrics:
            lines.extend(["", "### Improved Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in improved_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        if regressed_metrics:
            lines.extend(["", "### Regressed Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in regressed_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        top_improved = improvement.get("top_improved_functions") or []
        if top_improved:
            lines.extend(["", "### Improved Functions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improved[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {float(row.get('delta_percent', 0.0) or 0.0):+.3f}% | "
                    f"{float(row.get('baseline_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('current_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    lines.extend([
        "",
        "## By Language",
        "",
        "| Language | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
        "|---|---:|---:|---:|---:|---:|",
    ])
    for lang, bucket in sorted(summary["by_language"].items()):
        lines.append(
            f"| {lang} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
            f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
        )
    if summary.get("behavior_status_counts"):
        lines.extend(["", "## Behavior Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["behavior_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("decomp_cache_status_counts"):
        lines.extend(["", "## Decompile Cache Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["decomp_cache_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("decomp_failure_counts"):
        lines.extend(["", "## Decompile Failures", "", "| Failure | Rows |", "|---|---:|"])
        for failure, count in sorted(summary["decomp_failure_counts"].items()):
            lines.append(f"| {failure} | {count} |")
    if summary.get("debug_owner_bucket_counts"):
        lines.extend(["", "## Debug Owner Buckets", "", "| Bucket | Rows |", "|---|---:|"])
        for bucket, count in sorted(summary["debug_owner_bucket_counts"].items()):
            lines.append(f"| {bucket} | {count} |")
    if summary.get("debug_quality_evidence_totals"):
        lines.extend(["", "## Debug Quality Evidence", "", "| Metric | Total |", "|---|---:|"])
        for metric, total_value in sorted(summary["debug_quality_evidence_totals"].items()):
            lines.append(f"| {metric} | {total_value} |")
    if summary.get("debug_template_source_totals"):
        lines.extend(["", "## Debug SLEIGH Template Sources", "", "| Source | Total |", "|---|---:|"])
        for source, total_value in sorted(summary["debug_template_source_totals"].items()):
            lines.append(f"| {source} | {total_value} |")
    gate = summary.get("sleigh_template_source_gate")
    if isinstance(gate, dict):
        lines.extend(["", "## SLEIGH Template Source Gate", ""])
        lines.append(f"- Status: `{gate.get('status')}`")
        lines.append(f"- Required source: `{gate.get('required_source')}`")
        for failure in gate.get("failures") or []:
            lines.append(f"- Failure: {failure}")
    comparison = summary.get("comparison")
    if isinstance(comparison, dict):
        outcome = summary.get("comparison_outcome") if isinstance(summary.get("comparison_outcome"), dict) else {}
        weighted = comparison.get("metric_deltas", {}).get("weighted_semantic_similarity_percent", {})
        delta = weighted.get("delta")
        delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
        lines.extend(
            [
                "",
                "## Baseline Comparison",
                "",
                f"- Baseline: `{comparison.get('baseline_summary_path')}`",
                f"- Outcome: {outcome.get('headline', 'n/a')}",
                f"- Weighted semantic similarity delta: {delta_text}",
                f"- Improved rows: {comparison.get('improved_row_count', 0)}",
                f"- Regressed rows: {comparison.get('regressed_row_count', 0)}",
                f"- Behavior improved rows: {comparison.get('behavior_improved_row_count', 0)}",
                f"- Behavior regressed rows: {comparison.get('behavior_regressed_row_count', 0)}",
                f"- New rows: {comparison.get('new_row_count', 0)}",
                f"- Missing rows: {comparison.get('missing_row_count', 0)}",
            ]
        )
        top_improvements = comparison.get("top_improvements") or []
        if top_improvements:
            lines.extend(["", "### Top Improvements", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improvements[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_regressions = comparison.get("top_regressions") or []
        if top_regressions:
            lines.extend(["", "### Top Regressions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_regressions[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_deltas = comparison.get("top_row_deltas") or []
        if top_deltas:
            lines.extend(["", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_deltas[:10]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    debug_triage = summary.get("debug_triage") or []
    if debug_triage:
        lines.extend(["", "## Materialized Debug Triage", "", "| Function | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---|---|---|---|"])
        for row in debug_triage[:12]:
            lines.append(
                f"| `{row.get('function_name')}` | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    regression_debug_triage = summary.get("regression_debug_triage") or []
    if regression_debug_triage:
        lines.extend(["", "## Regression Debug Triage", "", "| Function | Delta | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---:|---|---|---|---|"])
        for row in regression_debug_triage[:12]:
            regression = row.get("baseline_regression") if isinstance(row.get("baseline_regression"), dict) else {}
            delta = regression.get("delta_percent")
            delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
            lines.append(
                f"| `{row.get('function_name')}` | {delta_text} | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    debug_commands = summary.get("debug_repro_commands") or []
    if debug_commands:
        lines.extend(["", "## Debug Repro Commands", ""])
        for row in debug_commands[:8]:
            lines.append(
                f"- `{row.get('entry_id')}` `{row.get('function_name')}` "
                f"({row.get('semantic_score_percent', 0.0):.3f}%, {row.get('behavior_status')}):"
            )
            lines.append("")
            lines.append("  ```bash")
            lines.append(f"  {row.get('debug_decomp_command')}")
            lines.append("  ```")
            if row.get("disasm_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('disasm_function_command')}")
                lines.append("  ```")
            if row.get("xrefs_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('xrefs_function_command')}")
                lines.append("  ```")
            if row.get("preview_candidate_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('preview_candidate_command')}")
                lines.append("  ```")
            if row.get("function_facts_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('function_facts_command')}")
                lines.append("  ```")
            if row.get("behavior_artifact_dir"):
                lines.append(f"  Behavior artifacts: `{row.get('behavior_artifact_dir')}`")
    failing = [row for row in rows if row.get("semantic_score", 0.0) < 1.0][:20]
    if failing:
        lines.extend(["", "## First Non-Perfect Rows", ""])
        for row in failing:
            behavior = row.get("behavior", {})
            lines.append(
                f"- `{row['entry_id']}` `{row['function_name']}`: score={row['semantic_score']:.3f}, "
                f"similarity={row['semantic_score_percent']:.3f}%, "
                f"map={row['mapping_status']}, behavior={behavior.get('status')}"
            )
            if behavior.get("artifact_dir"):
                lines.append(f"  behavior artifacts: `{behavior.get('artifact_dir')}`")
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
    decomp_cache: dict[str, dict[str, Any]],
    decomp_cache_lock: threading.Lock,
    decomp_cache_stats: Counter[str],
    include_debug_decomp: bool,
    output_dir: Path | None = None,
) -> dict[str, Any]:
    source_fp = code_fingerprint(func.body, func)
    mapping_status, matched, candidates = match_function(func, fission_funcs) if not fission_error else ("list_failed", None, [])
    decomp: dict[str, Any] = {"success": False, "failure_kind": mapping_status}
    if matched is not None:
        debug_decomp_bundle_path = (
            debug_bundle_path_for_parts(output_dir, entry.id, func.name, matched.address)
            if include_debug_decomp and output_dir is not None
            else None
        )
        decomp = run_fission_decomp_cached(
            entry.binary_path,
            matched.address,
            fission_bin,
            timeout_sec,
            include_debug_decomp,
            debug_decomp_bundle_path,
            decomp_cache,
            decomp_cache_lock,
            decomp_cache_stats,
        )
    else:
        decomp["decomp_cache_status"] = "not_requested"
    decomp_code = decomp.get("code") if decomp.get("success") else None
    decomp_fp = code_fingerprint(decomp_code or "") if decomp_code else Counter()
    static_score = multiset_jaccard(source_fp, decomp_fp) if decomp_code else 0.0
    behavior = run_behavior_check(
        entry,
        func,
        decomp_code,
        timeout_sec,
        host_execution,
        output_dir=output_dir,
        address=matched.address if matched else None,
    )
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
        "debug_decomp_bundle_path": decomp.get("debug_decomp_bundle_path"),
        "debug_decomp": decomp.get("debug_decomp"),
        "decomp_cache_status": decomp.get("decomp_cache_status", "not_requested"),
        "static_semantic_score": static_score,
        "static_semantic_score_percent": percent(static_score),
        "behavior": behavior,
        "semantic_score": semantic_score,
        "semantic_score_percent": percent(semantic_score),
    }


def run_benchmark(args: argparse.Namespace) -> int:
    start = time.perf_counter()
    created_at = utc_now()
    manifest_path = resolve_path(args.manifest)
    manifest = load_json(manifest_path)
    manifest_name = manifest.get("name", manifest_path.stem)
    run_id = f"{sanitize_id(manifest_name)}-{utc_timestamp_slug(created_at)}"
    entries = discover_source_entries(manifest)
    if args.limit_binaries is not None:
        entries = entries[: args.limit_binaries]

    output_dir = resolve_path(args.output_dir) if args.output_dir else DEFAULT_ARTIFACT_ROOT / run_id
    output_dir.mkdir(parents=True, exist_ok=True)
    fission_bin = resolve_path(args.fission_bin)
    host_execution = c_host_execution_probe(args.timeout_sec)

    rows: list[dict[str, Any]] = []
    jobs = max(1, int(args.jobs or 1))
    decomp_cache_path = None if args.no_decomp_cache else resolve_path(args.decomp_cache_file)
    decomp_cache: dict[str, dict[str, Any]] = load_decomp_cache(decomp_cache_path)
    decomp_cache_lock = threading.Lock()
    decomp_cache_stats: Counter[str] = Counter()
    decomp_cache_initial_entry_count = len(decomp_cache)
    list_cache_path = None if args.no_list_cache else resolve_path(args.list_cache_file)
    list_cache: dict[str, dict[str, Any]] = load_list_cache(list_cache_path)
    list_cache_stats: Counter[str] = Counter()
    list_cache_initial_entry_count = len(list_cache)
    for entry in entries:
        source_functions = extract_source_functions(entry.source_path, entry.language)
        fission_funcs, fission_error = run_fission_list_cached(
            entry.binary_path,
            fission_bin,
            args.timeout_sec,
            list_cache,
            list_cache_stats,
        )
        source_functions = select_source_functions(
            source_functions,
            fission_funcs,
            args.limit_functions,
            fission_error,
        )
        if jobs == 1 or len(source_functions) <= 1:
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
                        decomp_cache,
                        decomp_cache_lock,
                        decomp_cache_stats,
                        args.include_debug_decomp,
                        output_dir,
                    )
                )
            continue

        entry_rows: list[tuple[int, dict[str, Any]]] = []
        with ThreadPoolExecutor(max_workers=jobs) as executor:
            futures = {
                executor.submit(
                    row_for_function,
                    entry,
                    func,
                    fission_funcs,
                    fission_error,
                    fission_bin,
                    args.timeout_sec,
                    host_execution,
                    decomp_cache,
                    decomp_cache_lock,
                    decomp_cache_stats,
                    args.include_debug_decomp,
                    output_dir,
                ): index
                for index, func in enumerate(source_functions)
            }
            for future in as_completed(futures):
                entry_rows.append((futures[future], future.result()))
        rows.extend(row for _index, row in sorted(entry_rows, key=lambda item: item[0]))

    attach_debug_repro_commands(rows, fission_bin, output_dir)
    summary = summarize(rows, manifest_name, entries)
    summary["run_id"] = run_id
    summary["created_at_utc"] = utc_isoformat(created_at)
    summary["artifact_dir"] = rel(output_dir)
    summary["jobs"] = jobs
    summary["decomp_cache_file"] = rel(decomp_cache_path) if decomp_cache_path is not None else None
    summary["list_cache_file"] = rel(list_cache_path) if list_cache_path is not None else None
    summary["history_file"] = rel(DEFAULT_HISTORY_FILE)
    summary["latest_index_file"] = rel(DEFAULT_LATEST_INDEX_FILE)
    summary["decomp_cache_initial_entry_count"] = decomp_cache_initial_entry_count
    summary["decomp_cache_entry_count"] = len(decomp_cache)
    summary["decomp_cache_hit_count"] = int(decomp_cache_stats.get("hit", 0))
    summary["decomp_cache_miss_count"] = int(decomp_cache_stats.get("miss", 0))
    summary["decomp_cache_stored_count"] = int(decomp_cache_stats.get("stored", 0))
    summary["list_cache_initial_entry_count"] = list_cache_initial_entry_count
    summary["list_cache_entry_count"] = len(list_cache)
    summary["list_cache_hit_count"] = int(list_cache_stats.get("hit", 0))
    summary["list_cache_miss_count"] = int(list_cache_stats.get("miss", 0))
    summary["list_cache_stored_count"] = int(list_cache_stats.get("stored", 0))
    summary["wall_sec"] = round(time.perf_counter() - start, 6)
    if args.require_sleigh_template_source:
        summary["sleigh_template_source_gate"] = sleigh_template_source_gate(
            summary,
            args.require_sleigh_template_source,
        )
    history = history_snapshot(DEFAULT_HISTORY_FILE, summary)
    if history is not None:
        summary["history"] = history
    save_decomp_cache(decomp_cache_path, decomp_cache)
    save_list_cache(list_cache_path, list_cache)
    baseline_path: Path | None = None
    if not args.no_baseline_compare:
        baseline_path = resolve_path(args.baseline_dir) if args.baseline_dir else find_latest_baseline_dir(
            output_dir,
            summary["manifest"],
            {row_key(row) for row in rows},
        )
    if baseline_path is not None:
        try:
            baseline_summary, baseline_rows, baseline_summary_path = load_baseline_artifacts(baseline_path)
            summary["comparison"] = compare_to_baseline(
                summary,
                rows,
                baseline_summary,
                baseline_rows,
                baseline_summary_path,
            )
            summary["comparison_outcome"] = comparison_outcome(summary["comparison"])
            summary["improvement_summary"] = improvement_summary(summary["comparison"])
            if not args.no_baseline_snapshot:
                summary["baseline_snapshot"] = snapshot_baseline_artifacts(
                    output_dir,
                    baseline_summary_path,
                    baseline_summary,
                    baseline_rows,
                    summary["comparison"],
                )
        except Exception as exc:
            summary["comparison_error"] = {
                "baseline": str(baseline_path),
                "error": str(exc),
            }
    if args.materialize_debug_triage:
        triage_rows = materialize_debug_triage(
            rows,
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.debug_triage_limit,
        )
        summary["debug_triage"] = triage_rows
        summary["debug_triage_count"] = len(triage_rows)
    if args.materialize_regression_debug_triage and isinstance(summary.get("comparison"), dict):
        regression_triage_rows = materialize_regression_debug_triage(
            rows,
            summary["comparison"],
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.regression_debug_triage_limit,
        )
        summary["regression_debug_triage"] = regression_triage_rows
        summary["regression_debug_triage_count"] = len(regression_triage_rows)
    debug_commands = top_debug_commands(rows)
    if debug_commands:
        summary["debug_repro_commands"] = debug_commands
    (output_dir / "source_semantic_rows.json").write_text(
        dump_json_pretty(rows), encoding="utf-8"
    )
    (output_dir / "source_semantic_summary.json").write_text(
        dump_json_pretty(summary), encoding="utf-8"
    )
    if "comparison" in summary:
        (output_dir / "source_semantic_comparison.json").write_text(
            dump_json_pretty(summary["comparison"]), encoding="utf-8"
        )
    (output_dir / "source_semantic_summary.md").write_text(render_markdown(summary, rows), encoding="utf-8")
    gate = summary.get("sleigh_template_source_gate")
    gate_failed = isinstance(gate, dict) and gate.get("status") == "failed"
    if not gate_failed:
        append_history_record(DEFAULT_HISTORY_FILE, summary)
        update_latest_index(DEFAULT_LATEST_INDEX_FILE, summary)
    print(dump_json_pretty(summary), end="")
    return 1 if gate_failed else 0


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
        status, matched, candidates = match_function(
            SourceFunction(
                name="main",
                signature="int main()",
                body="return 0;",
                return_kind="int",
                param_kinds=[],
                param_names=[],
                line=1,
            ),
            [
                FissionFunction("0x1000", "__main"),
                FissionFunction("0x2000", "main"),
            ],
        )
        assert status == "matched"
        assert matched is not None
        assert matched.address == "0x2000"
        assert candidates == []
        limited = select_source_functions(
            [
                SourceFunction(
                    name="helper",
                    signature="static int helper(int x)",
                    body="return x + 1;",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=1,
                ),
                SourceFunction(
                    name="entry",
                    signature="int entry(int x)",
                    body="return helper(x);",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=2,
                ),
            ],
            [FissionFunction("0x3000", "entry")],
            1,
        )
        assert [func.name for func in limited] == ["entry"]
        assert classify_return("u64 wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        assert classify_return("uint64_t wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        void_func = SourceFunction(
            name="touch",
            signature="void touch(unsigned int seed)",
            body="control_sink = seed;",
            return_kind="void",
            param_kinds=["uint"],
            param_names=["seed"],
            line=1,
        )
        global_cases = [
            {
                "args": [7],
                "globals": [{"name": "control_sink", "ctype": "unsigned int", "reset": 0}],
            }
        ]
        valid, reason = validate_explicit_behavior_cases(void_func, global_cases)
        assert valid, reason
        rendered = render_explicit_case_call(void_func, global_cases[0], 0)
        assert "control_sink = 0;" in rendered
        assert "control_sink=%lld" in rendered
        assert "touch(7);" in rendered
        deduped_candidate = candidate_harness(
            "uint control_sink;\nuint math_sink;\nvoid touch(unsigned int seed) { control_sink = seed; }",
            void_func,
            global_cases,
        )
        assert "volatile unsigned int control_sink = 0;" in deduped_candidate
        assert "\nuint control_sink;\n" not in deduped_candidate
        assert "\nuint math_sink;\n" in deduped_candidate
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"raw_pcode:ok": 1},
                "debug_template_source_totals": {"spec_derived": 2},
            },
            "spec_derived",
        )
        assert gate["status"] == "passed"
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"raw_pcode:ok": 1},
                "debug_template_source_totals": {"compatibility_lowered": 1},
            },
            "spec_derived",
        )
        assert gate["status"] == "failed"
        assert "compatibility_lowered" in gate["failures"][0]
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {},
                "debug_template_source_totals": {},
            },
            "spec_derived",
        )
        assert gate["status"] == "failed"
        assert "--include-debug-decomp" in gate["failures"][0]
    print("self-test ok")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark Fission pseudocode against original source semantics. Ghidra is not used."
    )
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST), help="Source semantic manifest JSON")
    parser.add_argument("--fission-bin", default=str(DEFAULT_FISSION_BIN), help="Path to fission_cli")
    parser.add_argument(
        "--output-dir",
        help="Output artifact directory; defaults to a timestamped directory under benchmark/artifacts/source_semantic_benchmark",
    )
    parser.add_argument("--limit-binaries", type=int, help="Limit discovered manifest entries")
    parser.add_argument("--limit-functions", type=int, help="Limit source functions per entry")
    parser.add_argument("--timeout-sec", type=int, default=30, help="Per-command timeout")
    parser.add_argument(
        "--baseline-dir",
        help="Compare against a previous artifact directory or source_semantic_summary.json; defaults to latest matching prior run",
    )
    parser.add_argument(
        "--no-baseline-compare",
        action="store_true",
        help="Disable automatic comparison against previous source-semantic artifacts",
    )
    parser.add_argument(
        "--no-baseline-snapshot",
        action="store_true",
        help="Do not copy the selected baseline summary/rows/comparison into the current artifact directory",
    )
    parser.add_argument(
        "--include-debug-decomp",
        action="store_true",
        help="Pass fission_cli decomp --debug-decomp and attach compact stage/owner evidence to each row",
    )
    parser.add_argument(
        "--require-sleigh-template-source",
        choices=["spec_derived"],
        help=(
            "Fail the run unless all debug SLEIGH template-source evidence uses this source. "
            "Requires --include-debug-decomp for rows with raw_pcode:ok."
        ),
    )
    parser.add_argument(
        "--materialize-debug-triage",
        action="store_true",
        help="Run fission_cli decomp/disasm/xrefs/function-facts for the lowest-scoring rows and save captures",
    )
    parser.add_argument(
        "--debug-triage-limit",
        type=int,
        default=8,
        help="Maximum non-perfect rows to materialize with --materialize-debug-triage",
    )
    parser.add_argument(
        "--materialize-regression-debug-triage",
        action="store_true",
        help="Run fission_cli debug surfaces for rows that regressed versus the selected baseline",
    )
    parser.add_argument(
        "--regression-debug-triage-limit",
        type=int,
        default=8,
        help="Maximum regressed rows to materialize with --materialize-regression-debug-triage",
    )
    parser.add_argument(
        "--decomp-cache-file",
        default=str(DEFAULT_DECOMP_CACHE_FILE),
        help="Persistent decompile-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-decomp-cache",
        action="store_true",
        help="Disable the persistent decompile-result cache; the in-run memory cache remains enabled",
    )
    parser.add_argument(
        "--list-cache-file",
        default=str(DEFAULT_LIST_CACHE_FILE),
        help="Persistent fission_cli list-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-list-cache",
        action="store_true",
        help="Disable the persistent fission_cli list-result cache",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=DEFAULT_JOBS,
        help=f"Run source-function rows in parallel per binary entry (default: {DEFAULT_JOBS}; use 1 for serial)",
    )
    parser.add_argument("--self-test", action="store_true", help="Run lightweight parser/scoring self-test")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_benchmark(args)


if __name__ == "__main__":
    raise SystemExit(main())
